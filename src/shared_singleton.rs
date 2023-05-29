
#![allow(dead_code)]
use core::{cell::Cell, cell::UnsafeCell};
use core::mem::MaybeUninit;
use core::marker::Sync;

#[derive(Debug)]
pub enum ErrCode {
    NotOwned
}

#[derive(Copy, Clone, PartialEq)]
enum Owner {
    Vacant, // can be claimed for write
    Producer,  // claimed state
    Consumer,  // write done, passed to consumer
}

/// Single producer Single consumer Shared Singleton
/// Note that different from RefCell, the shared singleton cannot be read until
/// written by the producer
/// 
/// The inner UnsafeCell can be replaced by RefCell<T> is a much more sophisticated 
/// implementation with checks for multiple borrows. 
/// Here this version removes the safeguards assuming users handle the rest. The only protection
/// is the tristate owner flag which does not allow allocating for write more than once before
/// commit
pub struct SharedSingleton <T> {
    // TODO: enforce the owner field to entire word
    owner: Cell<Owner>,
    ucell: UnsafeCell<MaybeUninit<T>>,
}

// Delcare this is thread safe due to the owner protection
// sequence (Producer-> consumer , consumer -> owner)
unsafe impl <T> Sync for SharedSingleton<T> {}

impl <T> SharedSingleton<T> {
    
    const INIT_U: UnsafeCell<MaybeUninit<T>> = UnsafeCell::new(MaybeUninit::uninit());
    pub const INIT_0: SharedSingleton<T> = Self::new();

    #[inline]
    pub const fn new() -> Self {
        SharedSingleton { owner: Cell::new(Owner::Vacant), ucell: Self::INIT_U  }
    }

    #[inline]
    pub fn is_vacant(&self) -> bool {
        self.owner.get() == Owner::Vacant
    }

    /// Returns mutable reference of T if singleton is vacant
    #[inline]
    pub fn try_write(&self) -> Option<&mut T> {
        if self.owner.get() == Owner::Vacant {
            let x: *mut MaybeUninit<T> = self.ucell.get();
            let t: &mut T = unsafe {  &mut *(x as *mut T)};
            self.owner.set(Owner::Producer);
            Some(t)
        }
        else {
            None
        }
    }

    /// Pass ownership to Consumer from Producer
    #[inline]
    pub fn write_done(&self) -> Result<(),ErrCode> {
        if self.owner.get() == Owner::Producer {
            self.owner.set(Owner::Consumer);
            Ok(())
        }
        else {
            Err(ErrCode::NotOwned)
        }
    }

    /// Returns &T is location is owned by Consumer
    /// otherwise None
    /// NOTE: does not check for multiple calls
    #[inline]
    pub fn try_read(&self) -> Option<&T> {
        if self.owner.get() == Owner::Consumer {
            let x: *mut MaybeUninit<T> = self.ucell.get();
            let t: & T = unsafe {  & *(x as * const T)};
            Some(t)
        }
        else {
            None
        }
    }

    /// Release location back to Producer
    #[inline]
    pub fn read_done(&self) -> Result<(),ErrCode> {
        if self.owner.get() == Owner::Consumer {
            self.owner.set(Owner::Vacant);
            Ok(())
        }
        else {
            Err(ErrCode::NotOwned)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    
    pub struct SomeStruct {
        id: u32,
    }

    #[test]
    fn ownership_changes() {

        let shared = SharedSingleton::<SomeStruct>::new();

        if let Some(payload) = shared.try_write() {

            // Can only allocate once before commit
            assert!(shared.try_write().is_none());

            payload.id = 42;
            assert!(shared.write_done().is_ok());
        }

        // once passed to consumer, can't get mut_ref
        assert!(shared.try_write().is_none());

        assert!(shared.try_read().is_some());

        assert!(shared.try_read().unwrap().id == 42);

        assert!(shared.read_done().is_ok());

    }

}
