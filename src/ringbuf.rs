use crate::ringbuf_ref::{ErrCode, RingBufRef};
use core::cell::Cell;


pub struct Producer <'a,T, const N: usize> {

    ringbuf_ref: &'a RingBufRef<T, N>

}

impl<'a, T, const N: usize> Producer<'a, T, N> {

    
    pub fn alloc(&mut self) -> Result<&mut T, ErrCode> { 
        self.ringbuf_ref.alloc()
    }


    pub fn commit(&mut self) -> Result<(), ErrCode> { 
        self.ringbuf_ref.commit()
    }
}

pub struct Consumer <'a,T, const N: usize> {

    ringbuf_ref: &'a RingBufRef<T, N>

}

impl<'a, T, const N: usize> Consumer<'a, T, N> {

    pub fn peek(&self) -> Option<&T> {
        self.ringbuf_ref.peek()

    }
    
    pub fn peek_mut(&mut self) -> Option<&mut T> {

        self.ringbuf_ref.peek_mut()

    }


    pub fn pop(&mut self) -> Result<(), ErrCode> {
        self.ringbuf_ref.pop()
    }
}


pub struct RingBuf<T, const N: usize> {

    ringbuf_ref: RingBufRef<T, N>,
    has_split: Cell<bool>

}

impl<T, const N: usize> RingBuf<T, N> {

    pub const fn new() -> Self {
        RingBuf {
            ringbuf_ref: RingBufRef::new(),
            has_split: Cell::new(false)
        }
    }
    pub fn split(&self) -> Result<(Producer<'_, T, N>, Consumer<'_, T, N>), ()> {

        if self.has_split.get() {
            // Can only split once in life time
            Err(())
        }
        else {
            let producer = Producer {ringbuf_ref: &self.ringbuf_ref};
            let consumer = Consumer {ringbuf_ref: &self.ringbuf_ref};
            self.has_split.set(true);
            Ok((producer, consumer))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_mut_split() {
        
        let ringbuf = RingBuf::<u32, 4>::new();

        // The producer and consumer must be mutable to use the mut functions such
        // as alloc, commit and pop
        if let Ok((mut producer, mut consumer)) = ringbuf.split() {
            
            let loc = producer.alloc();

            if let Ok(v) = loc {
                *v = 42;

                assert!(producer.commit().is_ok());
            }

            assert!(consumer.peek().is_some());

            assert!(*consumer.peek().unwrap() == 42);

            assert!(consumer.pop().is_ok());

        }
        else {
            panic!("first split failed!");
        }

        assert!(ringbuf.split().is_err());
    }
}
