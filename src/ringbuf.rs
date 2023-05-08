use crate::ringbuf_ref::{ErrCode, RingBufRef};
use core::cell::Cell;


pub struct Producer <'a,T, const N: usize> {

    inner: &'a RingBufRef<T, N>

}

impl<'a, T, const N: usize> Producer<'a, T, N> {

    #[inline(always)]
    pub fn stage(&mut self) -> Option<&mut T> { 
        self.inner.stage()
    }


    #[inline(always)]
    pub fn commit(&mut self) -> Result<(), ErrCode> { 
        self.inner.commit()
    }
}

pub struct Consumer <'a,T, const N: usize> {

    inner: &'a RingBufRef<T, N>

}

impl<'a, T, const N: usize> Consumer<'a, T, N> {

    #[inline(always)]
    pub fn peek(&self) -> Option<&T> {
        self.inner.peek()

    }
    
    #[inline(always)]
    pub fn peek_mut(&mut self) -> Option<&mut T> {

        self.inner.peek_mut()

    }


    #[inline(always)]
    pub fn pop(&mut self) -> Result<(), ErrCode> {
        self.inner.pop()
    }
}


pub struct RingBuf<T, const N: usize> {

    ringbuf_ref: RingBufRef<T, N>,
    has_split_prod: Cell<bool>,
    has_split_cons: Cell<bool>

}

impl<T, const N: usize> RingBuf<T, N> {

    pub const INIT_0: RingBuf<T, N> = Self::new();

    pub const fn new() -> Self {
        RingBuf {
            ringbuf_ref: RingBufRef::new(),
            has_split_prod: Cell::new(false),
            has_split_cons: Cell::new(false)
        }
    }
    pub fn has_split_prod(&self) -> bool {
        self.has_split_prod.get()
    }
    pub fn has_split_cons(&self) -> bool {
        self.has_split_cons.get()
    }
    pub fn has_split(&self) -> bool {
        self.has_split_cons() || self.has_split_prod()
    }

    
    pub fn split_prod(&self) -> Result<Producer<'_, T, N>, ()> {

        if self.has_split_prod.get() {
            // Can only split once in life time
            Err(())
        }
        else {
            let producer = Producer {inner: &self.ringbuf_ref};
            self.has_split_prod.set(true);
            Ok(producer)
        }
    }
    pub fn split_cons(&self) -> Result<Consumer<'_, T, N>, ()> {

        if self.has_split_cons.get() {
            // Can only split once in life time
            Err(())
        }
        else {
            let consumer = Consumer {inner: &self.ringbuf_ref};
            self.has_split_cons.set(true);
            Ok(consumer)
        }
    }
    pub fn split(&self) -> Result<(Producer<'_, T, N>, Consumer<'_, T, N>), ()> {

        match (self.split_prod(), self.split_cons())  {
            (Ok(prod), Ok(cons)) => Ok((prod, cons)),
            _ => Err(())
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
        // as stage, commit and pop
        if let Ok((mut producer, mut consumer)) = ringbuf.split() {
            
            let loc = producer.stage();

            if let Some(v) = loc {
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
