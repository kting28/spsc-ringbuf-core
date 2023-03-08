//! Fixed capacity Single Producer Single Consumer Ringbuffer with no mutex protection.
//! Implementation based on https://www.snellman.net/blog/archive/2016-12-13-ring-buffers/

use core::mem::MaybeUninit;
use core::{cell::Cell, cell::UnsafeCell};

/// Internal Index struct emcapsulating masking and wrapping operations
/// according to size const size N. Note that we deliberately use u32
/// to limit the index to 4 bytes and max supported capacity to 2^31-1
#[derive(Eq, PartialEq)]
pub struct Index<const RANGE: usize> {
    cell: Cell<u32>,
}

#[derive(Debug)]
pub enum ErrCode {
    BuffFull,
    BuffEmpty,
}

impl<const N: usize> Index<N> {

    const OK: () = assert!(N < (u32::MAX/2) as usize, "Ringbuf capacity must be < u32::MAX/2");

    #[inline]
    pub fn wrap_inc(&self) {

        let n = N as u32;
        // Wrapping increment by 1 first
        let val = self.cell.get().wrapping_add(1);

        // Wrap index between [0, 2*N-1]
        // For power 2 of values, the natural overflow wrap
        // matches the wraparound of N. Hence the manual wrap
        // below is not required for power of 2 N
        if !n.is_power_of_two() && val > 2 * n - 1 {
            // val = val - 2*N
            self.cell.set(val.wrapping_sub(2 * n));
        } else {
            self.cell.set(val);
        }
    }
    #[inline]
    pub fn wrap_dist(&self, val: &Index<N>) -> u32 {
        
        let n = N as u32;
        // If N is power of two, just return wrapp_sub(val)
        // If N is not power of two, wrap value between [0, 2*N-1]
        // Assumes current value is in the range of [-2*N, 4*N-1]
        // Not asserting here since we only take Index, which cannot be
        // incremented beyong 2*N-1
        let raw = self.cell.get().wrapping_sub(val.get());
        if !n.is_power_of_two() {
            if (raw as i32) < 0 {
                return raw.wrapping_add(2 * n);
            } else if raw > 2 * n - 1 {
                return raw.wrapping_sub(2 * n);
            }
        }
        raw
    }

    // Mask the value for indexing [0, N-1]
    #[inline]
    pub fn mask(&self) -> u32 {
        let n = N as u32;
        let val = self.cell.get();
        if n.is_power_of_two() {
            val & (n - 1)
        } else if val > n - 1 {
            val - n
        } else {
            val
        }
    }

    #[inline]
    pub fn get(&self) -> u32 {
        self.cell.get()
    }
    
    #[allow(clippy::let_unit_value)]
    #[inline]
    pub const fn new(val: u32) -> Self {
        let _: () = Index::<N>::OK;
        Index {
            cell: Cell::new(val),
        }
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
unsafe impl<T, const N: usize> Sync for RingBufRef<T, N> {}

impl<T, const N: usize> RingBufRef<T, N> {
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
        RingBufRef {
            rd_idx: Index::new(0),
            wr_idx: Index::new(0),
            buffer_ucell: [Self::INIT_U; N],
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rd_idx == self.wr_idx
    }

    #[inline]
    pub fn len(&self) -> u32 {
        // returns the number of elements between read and write pointer
        // use wrapping sub
        self.wr_idx.wrap_dist(&self.rd_idx)
    }
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() as usize == N
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
    pub fn alloc(&self) -> Option<&mut T> {
        if !self.is_full() {
            // buffer_ucell contains UnsafeCell<MaybeUninit<T>>
            // UnsafeCell's get is defined as "fn get(&self) -> *mut T"
            let m: *mut MaybeUninit<T> = self.buffer_ucell[self.wr_idx.mask() as usize].get();
            let t: &mut T = unsafe { &mut *(m as *mut T) };
            Some(t)
        } else {
            None
        }
    }
    /// Commit whatever at the write index location by moving the write index
    #[inline]
    pub fn commit(&self) -> Result<(), ErrCode> {
        if !self.is_full() {
            self.wr_idx.wrap_inc();
            Ok(())
        } else {
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
            unsafe {
                (*self.buffer_ucell[self.wr_idx.mask() as usize].get()).write(val);
            }
            self.wr_idx.wrap_inc();
            Ok(())
        } else {
            Err(ErrCode::BuffFull)
        }
    }
    /// Returns an Option of reference to location at read index
    #[inline]
    pub fn peek(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            let x: *mut MaybeUninit<T> = self.buffer_ucell[self.rd_idx.mask() as usize].get();
            let t: &T = unsafe { &*(x as *const T) };
            Some(t)
        }
    }
    /// Returns an Option of mutable reference to location at read index
    #[inline]
    pub fn peek_mut(&self) -> Option<&mut T> {
        if self.is_empty() {
            None
        } else {
            let x: *mut MaybeUninit<T> = self.buffer_ucell[self.rd_idx.mask() as usize].get();
            let t: &mut T = unsafe { &mut *(x as *mut T) };
            Some(t)
        }
    }

    /// Consume the item at rd_idx
    #[inline]
    pub fn pop(&self) -> Result<(), ErrCode> {
        if !self.is_empty() {
            self.rd_idx.wrap_inc();
            Ok(())
        } else {
            Err(ErrCode::BuffEmpty)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl<T, const N: usize> RingBufRef<T, N> {
        
        // Test only method for testing wraparound
        // at extremes
        pub fn test_init_wr_rd(&self, val: u32) {
            self.wr_idx.cell.set(val);
            self.rd_idx.cell.set(val);
        }
    }
 
    // Test for static allocation
    // A 4-deep ring buffer
    const CMD_Q_DEPTH: usize = 4;
    pub struct SomeStruct {
        id: u32,
    }
    pub struct Interface {
        cmd_q: RingBufRef<SomeStruct, CMD_Q_DEPTH>,
    }

    // Set up 2 entries of interfaces
    const NUM_INTFS: usize = 2;
    // Init value, suppress the clippy warning for declaring const interior mutable "A “non-constant”
    // const item is a legacy way to supply an initialized value to downstream"
    #[allow(clippy::declare_interior_mutable_const)]
    const INTF_INIT: Interface = Interface {
        cmd_q: RingBufRef::INIT_0,
    };

    // Final instantiation as global
    static SHARED_INTF: [Interface; NUM_INTFS] = [INTF_INIT; NUM_INTFS];

    fn test_operations<const N: usize>(rbufr1: RingBufRef<u32, N>, iter: usize) {

        for i in 0..iter {
            let loc = rbufr1.alloc();

            if let Some(v) = loc {
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
            assert!(rbufr1.alloc().is_some());
            assert!(rbufr1.commit().is_ok());
        }
        // should fail
        assert!(rbufr1.alloc().is_none());
        assert!(rbufr1.commit().is_err());

        //println!("wr {} rd {}, len {}",
        //    rbufr1.wr_idx.cell.get(),
        //    rbufr1.rd_idx.cell.get(),
        //    rbufr1.len());

        // pop half
        for _ in 0..N / 2 {
            assert!(rbufr1.pop().is_ok());
        }
        //println!("wr {} rd {}, len {}",
        //    rbufr1.wr_idx.cell.get(),
        //    rbufr1.rd_idx.cell.get(),
        //    rbufr1.len());

        // alloc half
        for _ in 0..N / 2 {
            assert!(rbufr1.alloc().is_some());
            assert!(rbufr1.commit().is_ok());
        }
    }

    #[test]
    fn validate_size() {
        // 4 bytes of wr_idx, 4 bytes of rd_idx, 16*4 for buffer
        assert!(core::mem::size_of::<RingBufRef<u32, 16>>() == (4 + 4 + 16*4));

        // 4 bytes of wr_idx, 4 bytes of rd_idx, 16*2 for buffer
        assert!(core::mem::size_of::<RingBufRef<u16, 16>>() == (4 + 4 + 16*2));

        // 4 bytes of wr_idx, 4 bytes of rd_idx, 32*1 for buffer
        assert!(core::mem::size_of::<RingBufRef<u8, 32>>() == (4 + 4 + 32));
    }

    #[test]
    fn power_of_two_len() {
        let rbufr1: RingBufRef<u32, 16> = RingBufRef::new();
        test_operations::<16>(rbufr1, 2 * 16 - 1 + 16 / 2);
    }
    #[test]
    fn ping_pong() {
        let rbufr1: RingBufRef<u32, 2> = RingBufRef::new();
        test_operations::<2>(rbufr1, 2 * 2 - 1 + 2 / 2);
    }
    #[test]
    fn single() {
        let rbufr1: RingBufRef<u32, 1> = RingBufRef::new();
        test_operations::<1>(rbufr1, 7);
    }
    #[test]
    fn non_power_of_two_len() {
        let rbufr1: RingBufRef<u32, 15> = RingBufRef::new();
        test_operations::<15>(rbufr1, 2 * 15 - 1 + 15 / 2);
    }
    #[test]
    fn power_of_two_len_wrap() {
        // Test wr and rd near wraparound of u32
        let rbufr1: RingBufRef<u32, {(u16::MAX) as usize + 1}> = RingBufRef::new();
        // Caution - direct initialization must guarantee the value is valid
        // i.e. any value if N is power of 2; 
        // otherwise must be [0, 2*N-1]
        rbufr1.test_init_wr_rd(u32::MAX-2);
        test_operations::<{(u16::MAX) as usize + 1}>(rbufr1, 32768);
    }
    #[test]
    fn static_instance_example() {
        let intf: &'static Interface = &SHARED_INTF[0];

        let alloc_res = intf.cmd_q.alloc();

        if let Some(cmd) = alloc_res {
            cmd.id = 42;

            assert!(intf.cmd_q.commit().is_ok());
        }

        let cmd = intf.cmd_q.peek();

        assert!(cmd.is_some());
        assert!(cmd.unwrap().id == 42);
    }
    //#[test]
    //fn zero_len() {
    //    test_operations::<0>();
    //}
}
