
//! Fixed capacity Single Producer Single Consumer Ringbuffer with no mutex protection.
//! Implementation based on https://www.snellman.net/blog/archive/2016-12-13-ring-buffers/

use core::{cell::Cell, cell::UnsafeCell};
use core::mem::MaybeUninit;

/// Internal Index struct emcapsulating masking and wrapping operations
/// according to size const size N
#[derive(Eq, PartialEq)]
pub struct Index<const N: usize> {
    cell: Cell<usize>
}

#[derive(Debug)]
pub enum ErrCode {
    BuffFull,
    BuffEmpty
}

impl <const N: usize> Index<N> {

    #[inline]
    pub fn wrap_inc(&self) {

        // Wrapping increment by 1 first
        let val = self.cell.get().wrapping_add(1);

        // Wrap index between [0, 2*N-1]
        // For power 2 of values, the natural overflow wrap
        // matches the wraparound of N. Hence the manual wrap
        // below is not required for power of 2 N
        if !N.is_power_of_two() && val > 2*N-1 {
            // val = val - 2*N
            self.cell.set(val.wrapping_sub(2*N));
        }
        else {
            self.cell.set(val);
        }
    }
    #[inline]
    pub fn wrap_dist(&self, val: &Index<N> ) -> usize {
        // If N is power of two, just return wrapp_sub(val)
        // If N is not power of two, wrap value between [0, 2*N-1] 
        // Assumes current value is in the range of [-2*N, 4*N-1]
        // Not asserting here since we only take Index, which cannot be 
        // incremented beyong 2*N-1
        let raw = self.cell.get().wrapping_sub(val.get());
        if !N.is_power_of_two() {
            if (raw as i32) < 0 {
                return raw.wrapping_add(2*N)
            }
            else if raw > 2*N - 1 {
                return raw.wrapping_sub(2*N)
            }
        }
        raw
    }

    // Mask the value for indexing [0, N-1]
    #[inline]
    pub fn mask(&self) -> usize {
        let val = self.cell.get();
        if N.is_power_of_two() {
            val & (N-1)
        }
        else if val > N - 1 {
            val - N
        } 
        else {
            val
        }
    }

    #[inline]
    pub fn get(&self) -> usize {
        self.cell.get()
    }
    pub const fn new(val: usize) -> Self {
        Index { cell: Cell::new(val) }
    }
}

/// A ring buffer of capacity N holding items of type T.
/// Non power-of-two N is supported but less efficient.
pub struct RingBufRef<T, const N: usize> {
    // this is from where we dequeue items
    pub rd_idx: Index<N>,
    //  where we enqueue new items
    pub wr_idx: Index<N>,
    // this is the backend array
    buffer_ucell: [UnsafeCell<MaybeUninit<T>>; N],
}
// Delcare this is thread safe due to the owner protection
// sequence (Producer-> consumer , consumer -> owner)
unsafe impl <T, const N: usize> Sync for RingBufRef <T, N> {}

impl <T, const N: usize> RingBufRef<T, N> {

    // Need to prevent N = 0 instances since the code would compile but crash
    // on the 2*N-1 usize subtracts
    // https://users.rust-lang.org/t/how-do-i-static-assert-a-property-of-a-generic-u32-parameter/76307/2
    const OK: () = assert!(N > 0, "Ringbuf capacity must be larger than 0!");
    
    const INIT_U: UnsafeCell<MaybeUninit<T>> = UnsafeCell::new(MaybeUninit::uninit());
    pub const INIT_0: RingBufRef<T, N> = Self::new();

    #[allow(clippy::let_unit_value)]
    #[inline]
    pub const fn new() -> Self {
        // This dummy statement evaluates the assert to prevent 0 sized RingBufRef
        // from being compiled.
        let _: () = RingBufRef::<T, N>::OK;
        RingBufRef { rd_idx: Index::new(0), wr_idx: Index::new(0), buffer_ucell: [Self::INIT_U; N] }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rd_idx == self.wr_idx
    }

    #[inline]
    pub fn len(&self) -> usize {
        // returns the number of elements between read and write pointer
        // use wrapping sub
        self.wr_idx.wrap_dist(&self.rd_idx)
    }
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == N
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        N
    }

    /// Allocate means returning the write index location as mutable reference.
    /// The Result<> return enforces handling of return type
    /// I.e. if user does not check for push success, the compiler
    /// generates warnings
    /// Calling alloc twice without commit in between results in the same
    /// location written! We could add some protection by remembering this
    /// during alloc but this will incur runtime cost
    #[inline]
    pub fn alloc(&self) -> Result<&mut T, ErrCode> {
        if !self.is_full() {
            // buffer_ucell contains UnsafeCell<MaybeUninit<T>>
            // UnsafeCell's get is defined as "fn get(&self) -> *mut T"
            let m: *mut MaybeUninit<T> = self.buffer_ucell[self.wr_idx.mask()].get();
            let t: &mut T = unsafe {  &mut *(m as *mut T)};
            Ok(t)
        }
        else {
            Err(ErrCode::BuffFull)
        }
    }
    /// Commit whatever at the write index location by moving the write index
    #[inline]
    pub fn commit(&self) -> Result<(), ErrCode> {
        if !self.is_full() {
            self.wr_idx.wrap_inc();
            Ok(())
        }
        else {
            Err(ErrCode::BuffFull)
        }
    }

    /// Alloc and commit in one step by providing the value T to be written
    /// val's ownership is moved. (Question: it seems if T implements Clone,
    /// compiler copies T)
    #[inline]
    pub fn push(&self, val: T) -> Result<(), ErrCode> {
        if !self.is_full() {
            // buffer_ucell contains UnsafeCell<MaybeUninit<T>>
            // UnsafeCell's get is defined as "fn get(&self) -> *mut T"
            // * (* mut T) deference allows the MaybeUninit.write() to be called to 
            // Set the value
            unsafe {(*self.buffer_ucell[self.wr_idx.mask()].get()).write(val);}
            self.wr_idx.wrap_inc();
            Ok(())
        }
        else {
            Err(ErrCode::BuffFull)
        }
    }
    /// Returns an Option of reference to location at read index
    #[inline]
    pub fn peek(&self) -> Option<&T> {
        if self.is_empty() {
            None
        }
        else {
            let x: *mut MaybeUninit<T> = self.buffer_ucell[self.rd_idx.mask()].get();
            let t: &T = unsafe {  & *(x as *const T)};
            Some(t)
        }
    }
    /// Returns an Option of mutable reference to location at read index
    #[inline]
    pub fn peek_mut(&self) -> Option<&mut T> {
        if self.is_empty() {
            None
        }
        else {
            let x: *mut MaybeUninit<T> = self.buffer_ucell[self.rd_idx.mask()].get();
            let t: &mut T = unsafe {  &mut *(x as *mut T)};
            Some(t)
        }
    }

    /// Consume the item at rd_idx
    #[inline]
    pub fn pop(&self) -> Result<(), ErrCode> {
        if !self.is_empty() {
            self.rd_idx.wrap_inc();
            Ok(())
        }
        else {
            Err(ErrCode::BuffEmpty)
        }
        
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn test_operations<const N: usize> () {
        let rbufr1: RingBufRef<u32, N> = RingBufRef::new();

        for i in 0..2*N-1 {
            let loc = rbufr1.alloc();

            if let Ok(v) = loc {
                *v = i as u32;
            }
            assert!(rbufr1.commit().is_ok());
            if let Some(v) = rbufr1.peek() {
                assert!(*v == i as u32);
                assert!(rbufr1.pop().is_ok());
            }
        }

        assert!(rbufr1.peek().is_none());

        for _ in 0..N {
            assert!(rbufr1.alloc().is_ok());
            assert!(rbufr1.commit().is_ok());
        }
        // should fail
        assert!(rbufr1.alloc().is_err());
        assert!(rbufr1.commit().is_err());
    }

    #[test]
    fn power_of_two_len() {
        test_operations::<16>();
    }
    #[test]
    fn ping_pong() {
        test_operations::<2>();
    }
    #[test]
    fn single() {
        test_operations::<1>();
    }
    #[test]
    fn non_power_of_two_len() {
        test_operations::<15>();
    }
    //#[test]
    //fn zero_len() {
    //    test_operations::<0>();
    //}


}
