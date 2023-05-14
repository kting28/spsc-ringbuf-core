
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
    Unclaimed, // can be claimed for write
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
    pub ucell: UnsafeCell<MaybeUninit<T>>,
}

// Delcare this is thread safe due to the owner protection
// sequence (Producer-> consumer , consumer -> owner)
unsafe impl <T> Sync for SharedSingleton<T> {}

impl <T> SharedSingleton<T> {
    
    const INIT_U: UnsafeCell<MaybeUninit<T>> = UnsafeCell::new(MaybeUninit::uninit());
    pub const INIT_0: SharedSingleton<T> = Self::new();

    #[inline]
    pub const fn new() -> Self {
        SharedSingleton { owner: Cell::new(Owner::Unclaimed), ucell: Self::INIT_U  }
    }

    #[inline]
    pub fn is_vacant(&self) -> bool {
        self.owner.get() == Owner::Unclaimed
    }

    /// Returns mutable reference of T if singleton is owned by the producer
    #[inline]
    pub fn stage(&self) -> Option<&mut T> {
        if self.owner.get() == Owner::Unclaimed {
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
    pub fn commit(&self) -> Result<(),ErrCode> {
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
    pub fn peek(&self) -> Option<&T> {
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
    pub fn pop(&self) -> Result<(),ErrCode> {
        if self.owner.get() == Owner::Consumer {
            self.owner.set(Owner::Unclaimed);
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

        if let Some(payload) = shared.stage() {

            // Can only allocate once before commit
            assert!(shared.stage().is_none());

            payload.id = 42;
            assert!(shared.commit().is_ok());
        }

        // once passed to consumer, can't get mut_ref
        assert!(shared.stage().is_none());

        assert!(shared.peek().is_some());

        assert!(shared.peek().unwrap().id == 42);

        assert!(shared.pop().is_ok());

    }

}
