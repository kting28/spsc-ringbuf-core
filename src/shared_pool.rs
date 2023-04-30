use crate::ringbuf::{Consumer as RingBufConsumer, Producer as RingBufProducer, RingBuf};
use crate::shared_singleton::SharedSingleton;

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

pub trait HasPoolIdx<const N: usize> {
    fn get_pool_idx(&self) -> PoolIndex<N>;
    fn set_pool_idx(&mut self, pindex: PoolIndex<N>);
}

pub struct Producer<'a, T, Q: HasPoolIdx<N>, const N: usize> {
    // Producer handle for the command allocation
    pub alloc_prod: RingBufProducer<'a, Q, N>,
    // Consumer handle for the return ringbuf
    pub return_cons: RingBufConsumer<'a, Q, N>,
    // Reference to the payload pool
    pool_ref: &'a [SharedSingleton<T>; N],
}

impl<'a, T, Q: HasPoolIdx<N>, const N: usize> Producer<'a, T, Q, N> {
    pub const fn new(
        alloc_prod: RingBufProducer<'a, Q, N>,
        return_cons: RingBufConsumer<'a, Q, N>,
        pool_ref: &'a [SharedSingleton<T>; N],
    ) -> Self {
        Producer {
            alloc_prod,
            return_cons,
            pool_ref,
        }
    }

    // Internal - get an item from the pool
    fn get_pool_item(&mut self) -> PoolIndex<N> {
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

    // Allocate queue without payload
    pub fn alloc(&mut self) -> Option<&mut Q> {
        if let Some(item) = self.alloc_prod.alloc() {
            item.set_pool_idx(PoolIndex::<N>(N as u32));

            Some(item)
        } else {
            None
        }
    }

    // Allocate a command buffer and an accompanying payload from the pool
    // Return a pair of mutable references if successful
    pub fn alloc_with_payload(&mut self) -> Result<(&mut Q, &SharedSingleton<T>), SharedPoolError> {
        if let Ok(idx) = usize::try_from(self.get_pool_item()) {
            let payload = &self.pool_ref[idx];

            if let Some(item) = self.alloc_prod.alloc() {
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
        if let Some(item) = self.alloc_prod.alloc() {
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

pub struct Consumer<'a, T, Q: HasPoolIdx<N>, const N: usize> {
    // Consumer handle for the command allocation
    pub alloc_cons: RingBufConsumer<'a, Q, N>,
    // Producer handle for the return ringbuf
    pub return_prod: RingBufProducer<'a, Q, N>,
    // Reference to the payload pool
    pool_ref: &'a [SharedSingleton<T>; N],
}

impl<'a, T, Q: HasPoolIdx<N>, const N: usize> Consumer<'a, T, Q, N> {
    pub fn peek(&self) -> Option<&Q> {
        self.alloc_cons.peek()
    }

    pub fn pop(&mut self) -> Result<(), SharedPoolError> {
        self.alloc_cons
            .pop()
            .map_err(|_| SharedPoolError::AllocBufEmpty)
    }

    // Return a payload location in the pool back to the Producer
    pub fn enqueue_return(&mut self, pidx: PoolIndex<N>) -> Result<(), SharedPoolError> {
        // Allocation a location in the return queue
        if let Some(re) = self.return_prod.alloc() {
            // Assert returned payload idx is at least valid
            // That's the best we can do from consumer side
            let loc = usize::try_from(pidx).unwrap();

            assert!(self.pool_ref[loc].is_vacant());

            re.set_pool_idx(pidx);

            self.return_prod
                .commit()
                .map_err(|_| SharedPoolError::ReturnBufFull)
        } else {
            Err(SharedPoolError::ReturnBufFull)
        }
    }
}

pub struct SharedPool<T, Q: HasPoolIdx<N>, const N: usize> {
    alloc_rbuf: RingBuf<Q, N>,
    return_rbuf: RingBuf<Q, N>,
    pool: [SharedSingleton<T>; N],
}

unsafe impl<T, Q: HasPoolIdx<N>, const N: usize> Sync for SharedPool<T, Q, N> {}

impl<T, Q: HasPoolIdx<N>, const N: usize> SharedPool<T, Q, N> {
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

    pub fn split(&self) -> Result<(Producer<'_, T, Q, N>, Consumer<'_, T, Q, N>), SharedPoolError> {
        if self.alloc_rbuf.has_split() || self.return_rbuf.has_split() {
            // Can only split once in life time
            Err(SharedPoolError::AlreadySplit)
        } else {
            // Split the allocation and return ring buffers to their
            // corresponding producers and consumers. Not expected to fail
            // since this is already protected by our own has split flag
            let (alloc_p, alloc_c) = self.alloc_rbuf.split().unwrap();
            let (mut ret_p, ret_c) = self.return_rbuf.split().unwrap();

            // Pre-fill the return queue with all the pool indices
            for i in 0..N {
                // Can unwrap here as we don't expect this fail
                let item = ret_p.alloc().unwrap();
                item.set_pool_idx(PoolIndex(i as u32));
                ret_p.commit().unwrap();
            }

            // Distribute the producers and consumers to the final
            // Producer and Consumer wrappers
            let producer = Producer {
                alloc_prod: alloc_p,
                return_cons: ret_c,
                pool_ref: &self.pool,
            };
            let consumer = Consumer {
                alloc_cons: alloc_c,
                return_prod: ret_p,
                pool_ref: &self.pool,
            };
            Ok((producer, consumer))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD_Q_DEPTH: usize = 16;
    pub struct Message {
        id: u32,
        payload: PoolIndex<CMD_Q_DEPTH>,
    }

    impl HasPoolIdx<CMD_Q_DEPTH> for Message {
        fn get_pool_idx(&self) -> PoolIndex<CMD_Q_DEPTH> {
            self.payload
        }
        fn set_pool_idx(&mut self, pindex: PoolIndex<CMD_Q_DEPTH>) {
            self.payload = pindex
        }
    }

    pub struct Payload {
        value: u32,
    }

    static SHARED_POOL: SharedPool<Payload, Message, 16> = SharedPool {
        alloc_rbuf: RingBuf::INIT_0,
        return_rbuf: RingBuf::INIT_0,
        pool: [SharedSingleton::<Payload>::INIT_0; 16],
    };

    #[test]
    fn test_sanity() {
        if let Ok((mut producer, mut consumer)) = SHARED_POOL.split() {

            // Try to allocate a pool item
            let payload_idx = producer.get_pool_item();

            // Assert allocation is successful
            assert!(usize::try_from(payload_idx).is_ok());

            let pool_idx: usize = payload_idx.try_into().unwrap();

            //allocate the location for write, this changes the owner
            //to Producer
            assert!(producer.pool_ref[pool_idx].alloc().is_some());

            // Commit it
            assert!(producer.pool_ref[pool_idx].commit().is_ok());

            // Allocate the actual command
            let message = producer.alloc().unwrap();

            // Set the payload index to the message
            message.set_pool_idx(payload_idx);

            // Commit the command message
            assert!(producer.commit().is_ok());

            // Test consumer
            assert!(consumer.alloc_cons.peek().is_some());

            let recvd = consumer.alloc_cons.peek().unwrap();

            let ret_pool_idx: usize = recvd.get_pool_idx().try_into().unwrap();

            // Return the payload item to the pool
            assert!(consumer.pool_ref[ret_pool_idx].pop().is_ok());

            assert!(consumer.enqueue_return(recvd.get_pool_idx()).is_ok());


        } else {
            panic!("first split failed!");
        }
    }
}
