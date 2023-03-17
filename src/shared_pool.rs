
use crate::ringbuf::{RingBuf, Producer as RingBufProducer};
use crate::shared_singleton::SharedSingleton;

pub enum SharedPoolError {

    PoolFull,
    AllocBufFull,
    ReturnBufFull
}

pub struct PoolIndex<const N: usize> {
    index: usize,
}


//impl TryFrom<i32> for GreaterThanZero {
//    type Error = &'static str;
//
//    fn try_from(value: i32) -> Result<Self, Self::Error> {
//        if value <= 0 {
//            Err("GreaterThanZero only accepts values greater than zero!")
//        } else {
//            Ok(GreaterThanZero(value))
//        }
//    }
//}


impl<const N: usize> PoolIndex<N> {

    pub fn access(&self) -> Option<usize> {

        if self.index < N {
            Some(self.index)
        }
        else {
            None
        }
    }

    pub const fn new(index: usize) -> Self {
        Self { index }
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

pub struct Producer <'a, T, Q: HasPayload<N>, const N: usize, const BMAP_LEN: usize> {

    rbuf_producer: RingBufProducer<'a, Q, N>,
    pool_ref: &'a [SharedSingleton<T>; N],
    free_bitmap: [u32; BMAP_LEN]

}

impl<'a, T, Q: HasPayload<N>, const N: usize, const BMAP_LEN: usize> Producer<'a, T, Q, N, BMAP_LEN> {

    const BMAP_LEN_DERIVED : usize = (N+31)/32;

    pub const fn new(rbuf_producer: RingBufProducer<'a, Q, N>, 
                     pool_ref: &'a [SharedSingleton<T>; N]) -> Self {
        assert!(Self::BMAP_LEN_DERIVED == BMAP_LEN, "Supplied bitmap len does not cover N");
        let mut free_bitmap: [u32; BMAP_LEN] = [0xFFFFFFFF; BMAP_LEN];
        if BMAP_LEN % 32 != 0 {
            free_bitmap[BMAP_LEN-1] = (1<< (BMAP_LEN %32)) - 1;
        }
        Producer {
            rbuf_producer,
            pool_ref,
            free_bitmap
        }
    }

    pub fn alloc_payload(&mut self) -> PoolIndex<N> {

        let mut index = N;

        for i in 0..self.pool_ref.len() {

            let mut limit = 32;

            if i == self.pool_ref.len()-1 {
                limit = i % 32;
            }

            if self.free_bitmap[i].trailing_zeros() < limit as u32 {

                // Clear the left-most bit
                self.free_bitmap[i] &= self.free_bitmap[i].wrapping_sub(1);

                index = i;
            }
        }
        PoolIndex::new(index)
    }

    pub fn alloc(&mut self) -> Option<&mut Q> {
        self.rbuf_producer.alloc()
    }

    pub fn alloc_with_payload(&mut self) -> Result< (&mut Q, &SharedSingleton<T>), SharedPoolError> {

        if let Some(idx) = self.alloc_payload().access() {

            let payload = &self.pool_ref[idx];

            if let Some(item) = self.alloc() {

                item.set_load_idx(PoolIndex::<N>::new(idx));
                
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

    pub fn commit(&mut self) -> Result<(), ()> {

        // commit the payload if attached
        // commit the command queue
        Ok(())
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


