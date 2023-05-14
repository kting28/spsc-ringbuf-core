use crate::ringbuf::{Consumer as RingBufConsumer, Producer as RingBufProducer, RingBuf};
use crate::shared_singleton::SharedSingleton;

#[derive(Debug)]
pub enum SharedPoolError {
    PoolFull,
    AllocBufFull,
    ReturnBufFull,
    AllocBufEmpty,
    PayloadNotConsumerOwned,
    AlreadySplit,
}

#[derive(Clone, Copy)]
pub struct PoolIndex<const N: usize>(u32);

// Get usize from PoolIndex<N>
impl<const N: usize> TryFrom<PoolIndex<N>> for usize {
    type Error = ();

    fn try_from(value: PoolIndex<N>) -> Result<Self, Self::Error> {
        if value.0 >= N as u32 {
            // Invalid, cannot be referenced
            Err(())
        } else {
            // Ok, can be referenced
            Ok(value.0 as usize)
        }
    }
}

impl<const N: usize>  PoolIndex<N> {
    pub fn is_valid(&self) -> bool {
        self.0 < N as u32
    }
}

pub trait HasPoolIdx<const N: usize> {
    fn get_pool_idx(&self) -> PoolIndex<N>;
    fn set_pool_idx(&mut self, pindex: PoolIndex<N>);
}

pub struct Producer<'a, T, Q: HasPoolIdx<N>, const N: usize, const M: usize> {
    // Producer handle for the command allocation
    pub alloc_prod: RingBufProducer<'a, Q, M>,
    // Consumer handle for the return ringbuf
    pub return_cons: RingBufConsumer<'a, Q, M>,
    // Reference to the payload pool
    pool_ref: &'a [SharedSingleton<T>; N],
}

impl<'a, T, Q: HasPoolIdx<N>, const N: usize, const M: usize> Producer<'a, T, Q, N, M> {
    pub const fn new(
        alloc_prod: RingBufProducer<'a, Q, M>,
        return_cons: RingBufConsumer<'a, Q, M>,
        pool_ref: &'a [SharedSingleton<T>; N],
    ) -> Self {
        Producer {
            alloc_prod,
            return_cons,
            pool_ref,
        }
    }

    // Internal - get an item from the pool
    fn take_pool_item(&mut self) -> PoolIndex<N> {
        // Check the return queue
        if let Some(item) = self.return_cons.peek() {
            // If there's a return item it must be a valid
            // pool index
            let payload_idx = usize::try_from(item.get_pool_idx()).unwrap();

            // Assert location indicated as free is actually vacant
            assert!(self.pool_ref[payload_idx].is_vacant());

            // Pop the return queue
            assert!(self.return_cons.pop().is_ok());

            return PoolIndex(payload_idx as u32);
        }
        // Otherwise nothing is valid
        PoolIndex(N as u32)
    }

    // Stage item for write without payload
    pub fn stage(&mut self) -> Option<&mut Q> {
        if let Some(item) = self.alloc_prod.stage() {
            item.set_pool_idx(PoolIndex::<N>(N as u32));

            Some(item)
        } else {
            None
        }
    }

    // Stage a command buffer and an accompanying payload from the pool
    // Return a pair of mutable references if successful
    pub fn stage_with_payload(&mut self) -> Result<(&mut Q, &SharedSingleton<T>), SharedPoolError> {
        if let Ok(idx) = usize::try_from(self.take_pool_item()) {
            let payload = &self.pool_ref[idx];

            if let Some(item) = self.alloc_prod.stage() {
                item.set_pool_idx(PoolIndex::<N>(idx as u32));

                Ok((item, payload))
            } else {
                Err(SharedPoolError::AllocBufFull)
            }
        } else {
            Err(SharedPoolError::PoolFull)
        }
    }

    // Commit the command. If command can contain payload, check
    // if the payload has already been passed to the consumer.
    pub fn commit(&mut self) -> Result<(), SharedPoolError> {
        // In payload has been allocated, check if passed to consumer.
        if let Some(item) = self.alloc_prod.stage() {
            if let Ok(idx) = usize::try_from(item.get_pool_idx()) {
                if self.pool_ref[idx].peek().is_none() {
                    // Payload index is set but not passed to consumer
                    return Err(SharedPoolError::PayloadNotConsumerOwned);
                }
            }
        }
        // commit the command queue. Map the only possible commit error (BufFull)
        // to SharedPoolError::AllocBufFull
        self.alloc_prod
            .commit()
            .map_err(|_| SharedPoolError::AllocBufFull)
    }
}

pub struct Consumer<'a, T, Q: HasPoolIdx<N>, const N: usize, const M: usize> {
    // Consumer handle for the command allocation
    pub alloc_cons: RingBufConsumer<'a, Q, M>,
    // Producer handle for the return ringbuf
    pub return_prod: RingBufProducer<'a, Q, M>,
    // Reference to the payload pool
    pool_ref: &'a [SharedSingleton<T>; N],
}

impl<'a, T, Q: HasPoolIdx<N>, const N: usize, const M: usize> Consumer<'a, T, Q, N, M> {
    pub fn peek(&self) -> (Option<&Q>, Option<&SharedSingleton<T>>) {
        let ret = self.alloc_cons.peek();

        match ret {
            Some(message) => {
                let has_idx = message.get_pool_idx();
                if let Ok(idx) = usize::try_from(has_idx) {
                    (ret, Some(&self.pool_ref[idx]))
                }
                else {
                    (ret, None)
                }
            }
            _ => (None, None)
        }
    }

    pub fn pop(&mut self) -> Result<(), SharedPoolError> {
        self.alloc_cons
            .pop()
            .map_err(|_| SharedPoolError::AllocBufEmpty)
    }

    // Return a payload location in the pool back to the Producer
    pub fn enqueue_return(&mut self, pidx: PoolIndex<N>) -> Result<(), SharedPoolError> {
        // Allocation a location in the return queue
        if let Some(re) = self.return_prod.stage() {
            // Assert returned payload idx is at least valid
            // That's the best we can do from consumer side
            assert!(pidx.is_valid());

            // pidx is asserted above to be valid
            // pidx.0 is private, hence user cannot access the value
            // directly. Also pool_ref is private
            assert!(self.pool_ref[pidx.0 as usize].is_vacant());

            re.set_pool_idx(pidx);

            self.return_prod
                .commit()
                .map_err(|_| SharedPoolError::ReturnBufFull)
        } else {
            Err(SharedPoolError::ReturnBufFull)
        }
    }
}

pub struct SharedPool<T, Q: HasPoolIdx<N>, const N: usize, const M: usize> {
    alloc_rbuf: RingBuf<Q, M>,
    return_rbuf: RingBuf<Q, M>,
    pool: [SharedSingleton<T>; N],
}

unsafe impl<T, Q: HasPoolIdx<N>, const N: usize, const M: usize> Sync for SharedPool<T, Q, N, M> {}

impl<T, Q: HasPoolIdx<N>, const N: usize, const M: usize> SharedPool<T, Q, N, M> {
    // new
    // initialize return_rbuf to be full
    // return to be empty

    pub const fn new() -> Self {
        SharedPool {
            alloc_rbuf: RingBuf::new(),
            return_rbuf: RingBuf::new(),
            pool: [SharedSingleton::INIT_0; N],
        }
    }

    // Return the producer, once in life time
    pub fn split_prod(&self) -> Result<Producer<'_, T, Q, N, M>, SharedPoolError> {
        if self.alloc_rbuf.has_split_prod() || self.return_rbuf.has_split_cons() {
            // Can only split once in life time
            Err(SharedPoolError::AlreadySplit)
        } else {
            // Split the allocation and return ring buffers to their
            // corresponding producers and consumers. Not expected to fail
            // since this is already protected by our own has split flag
            let alloc_p = self.alloc_rbuf.split_prod().unwrap();
            let ret_c = self.return_rbuf.split_cons().unwrap();

            // Distribute the producers and consumers to the final
            // Producer and Consumer wrappers
            let producer = Producer {
                alloc_prod: alloc_p,
                return_cons: ret_c,
                pool_ref: &self.pool,
            };
            Ok(producer)
        }
    }

    // Return the consumer, once in life time
    pub fn split_cons(&self) -> Result<Consumer<'_, T, Q, N, M>, SharedPoolError> {
        if self.alloc_rbuf.has_split_cons() || self.return_rbuf.has_split_prod() {
            // Can only split once in life time
            Err(SharedPoolError::AlreadySplit)
        } else {
            // Split the allocation and return ring buffers to their
            // corresponding producers and consumers. Not expected to fail
            // since this is already protected by our own has split flag
            let alloc_c = self.alloc_rbuf.split_cons().unwrap();
            let mut ret_p = self.return_rbuf.split_prod().unwrap();

            // Pre-fill the return queue with all the pool indices
            for i in 0..N {
                // Can unwrap here as we don't expect this fail
                let item = ret_p.stage().unwrap();
                item.set_pool_idx(PoolIndex(i as u32));
                ret_p.commit().unwrap();
            }

            let consumer = Consumer {
                alloc_cons: alloc_c,
                return_prod: ret_p,
                pool_ref: &self.pool,
            };
            Ok(consumer)
        }
    }
    // Split both producer and consumer handle together
    pub fn split(&self) -> Result<(Producer<'_, T, Q, N, M>, Consumer<'_, T, Q, N, M>), SharedPoolError> {

        match (self.split_prod(), self.split_cons())  {
            (Ok(prod), Ok(cons)) => Ok((prod, cons)),
            _ => Err(SharedPoolError::AlreadySplit)
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    const POOL_DEPTH: usize = 16;
    pub struct Message {
        id: u32,
        payload: PoolIndex<POOL_DEPTH>,
    }

    impl HasPoolIdx<POOL_DEPTH> for Message {
        fn get_pool_idx(&self) -> PoolIndex<POOL_DEPTH> {
            self.payload
        }
        fn set_pool_idx(&mut self, pindex: PoolIndex<POOL_DEPTH>) {
            self.payload = pindex
        }
    }

    pub struct Payload {
        value: u32,
    }

    static SHARED_POOL: SharedPool<Payload, Message, 16, 32> = SharedPool {
        alloc_rbuf: RingBuf::INIT_0,
        return_rbuf: RingBuf::INIT_0,
        pool: [SharedSingleton::<Payload>::INIT_0; 16],
    };

    #[test]
    fn test_static() {
        if let Ok((mut producer, mut consumer)) = SHARED_POOL.split() {

            // Allocate the actual command
            let (message, payload) = producer.stage_with_payload().unwrap();
            
            // Update the message
            message.id = 41;
            let raw = payload.stage().unwrap();
            raw.value = 42;
            // Pass the payload
            payload.commit().unwrap();

            // Commit 
            assert!(producer.commit().is_ok());

            // Test consumer can see it
            assert!(consumer.peek().0.is_some());

            let (recvd, payload) = consumer.peek();

            assert!(recvd.unwrap().id == 41);

            assert!(payload.unwrap().peek().unwrap().value == 42);

            // Return the payload item to producer
            assert!(payload.unwrap().pop().is_ok());

            // Return the payload location back to the queue
            assert!(consumer.enqueue_return(recvd.unwrap().get_pool_idx()).is_ok());

        } else {
            panic!("first split failed!");
        }
    }


}
