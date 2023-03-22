
use crate::ringbuf::{RingBuf, Producer as RingBufProducer, Consumer as RingBufConsumer};
use crate::shared_singleton::SharedSingleton;
use crate::ringbuf_ref::ErrCode;

pub enum SharedPoolError {

    PoolFull,
    AllocBufFull,
    ReturnBufFull,
    AllocBufEmpty,
    InvalidPayload

}

#[derive(Clone, Copy)]
pub struct PoolIndex<const N: usize>(u32);

// Get usize from PoolIndex<N>
impl<const N: usize> TryFrom<PoolIndex<N>> for usize {
    type Error = ();

    fn try_from(value: PoolIndex<N>) -> Result<Self, Self::Error> {
        if value.0 >= N as u32{
            // Invalid, cannot be referenced
            Err(())
        } else {
            // Ok, can be referenced
            Ok(value.0 as usize)
        }
    }
}

// Get a pair of word offset and bit location from PoolIndex
impl<const N: usize> TryFrom<PoolIndex<N>> for (usize, usize) {

    type Error = ();

    fn try_from(value: PoolIndex<N>) -> Result<Self, Self::Error> {
        if value.0 >= N as u32{
            // Invalid, cannot be referenced
            Err(())
        } else {
            // Ok, can be referenced
            Ok((N/32, N%32))
        }
    }
}

pub trait HasPayload<const N: usize> {

    fn payload_idx(&self) -> PoolIndex<N>;

    fn set_load_idx(&mut self, pindex: PoolIndex<N>);

}

pub struct SharedPool<T, Q: HasPayload<N>, const N: usize> {

    alloc_rbuf: RingBuf<Q, N>,
    return_rbuf: RingBuf<Q, N>,
    pool: [SharedSingleton<T>; N]
}

pub struct Producer <'a, T, Q: HasPayload<N>, const N: usize> {

    // Producer handle for the command allocation
    alloc_prod: RingBufProducer<'a, Q, N>,
    // Consumer handle for the return ringbuf
    return_cons: RingBufConsumer<'a, Q, N>,
    // Reference to the payload pool
    pool_ref: &'a [SharedSingleton<T>; N],

}

impl<'a, T, Q: HasPayload<N>, const N: usize> Producer<'a, T, Q, N> {

    pub const fn new(
                     alloc_prod: RingBufProducer<'a, Q, N>, 
                     return_cons: RingBufConsumer<'a, Q, N>, 
                     pool_ref: &'a [SharedSingleton<T>; N]) -> Self 
    {
        Producer {
            alloc_prod,
            return_cons,
            pool_ref
        }
    }


    // Allocate a payload item from the return rbuf
    pub fn alloc_payload(&mut self) -> PoolIndex<N> {

        // Check the return queue
        if let Some(item) = self.return_cons.peek() {
        
            // If there's a return item it must be a valid 
            // pool index
            let payload_idx = usize::try_from(item.payload_idx()).unwrap();

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

            item.set_load_idx(PoolIndex::<N>(N as u32));

            Some(item)
        }
        else {
            None
        }
    }

    // Allocate a command buffer and an accompanying payload from the pool
    // Return a pair of mutable references if successful
    pub fn alloc_with_payload(&mut self) -> Result< (&mut Q, &SharedSingleton<T>), SharedPoolError> {

        if let Ok(idx) = usize::try_from(self.alloc_payload()) {

            let payload = &self.pool_ref[idx];

            if let Some(item) = self.alloc_prod.alloc() {

                item.set_load_idx(PoolIndex::<N>(idx as u32));
                
                Ok((item, payload))
            }
            else {
                Err(SharedPoolError::AllocBufFull)
            }
        }
        else {
            Err(SharedPoolError::PoolFull)
        }
    }

    // Commit the command. If command can contain payload, check
    // if the payload has already been passed to the consumer.
    pub fn commit(&mut self) -> Result<(), SharedPoolError> {

        // In payload has been allocated, check if passed to consumer.
        if let Some(item) = self.alloc_prod.alloc() {
            if let Ok(idx) = usize::try_from(item.payload_idx()) {
                if self.pool_ref[idx].peek().is_none() {
                    // Payload index is set but not passed to consumer
                    return Err(SharedPoolError::InvalidPayload);
                }
            }
        }
        // commit the command queue. Map the only possible commit error (BufFull)
        // to SharedPoolError::AllocBufFull
        self.alloc_prod.commit().map_err(|_| SharedPoolError::AllocBufFull ) 
    }
}


pub struct Consumer <'a, T, Q: HasPayload<N>, const N: usize> {

    // Consumer handle for the command allocation
    alloc_cons: RingBufConsumer<'a, Q, N>,
    // Producer handle for the return ringbuf
    return_prod: RingBufProducer<'a, Q, N>,
    // Reference to the payload pool
    pool_ref: &'a [SharedSingleton<T>; N],

}


impl<'a, T, Q: HasPayload<N>, const N: usize> Consumer<'a, T, Q, N> {

    pub fn peek(&self) -> Option<&Q> {

        self.alloc_cons.peek()
    }

    pub fn pop(&mut self) ->  Result<(), SharedPoolError> {

        self.alloc_cons.pop().map_err(|_| SharedPoolError::AllocBufEmpty)

    }

    // Return a payload location in the pool back to the Producer
    pub fn return_payload(&mut self, pidx: PoolIndex<N>) -> Result<(), SharedPoolError> {
        
        // Allocation a location in the return queue
        if let Some(re) = self.return_prod.alloc() {

            // Assert returned payload idx is at least valid
            // That's the best we can do from consumer side
            let loc = usize::try_from(pidx).unwrap();

            assert!(self.pool_ref[loc].is_vacant());

            re.set_load_idx(pidx);

            self.return_prod.commit().map_err(|_| SharedPoolError::ReturnBufFull)
        }
        else  {
            Err(SharedPoolError::ReturnBufFull)
        }
    }
}


impl<T, Q: HasPayload<N>, const N: usize> SharedPool<T, Q, N> {

    // new
    // initialize alloc_rbuf to be full
    // return to be empty
    
    // alloc 
    // if alloc_rbuf full , collect all from return_rbuf
    // alloc and return


}


