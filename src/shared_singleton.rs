
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
    Producer,
    Consumer,
}

/// Single producer Single consumer Shared Singleton
/// Note that different from RefCell, the shared singleton cannot be read until
/// written by the producer
/// 
/// The inner UnsafeCell can be replaced by RefCell<T> is a much more sophisticated 
/// implementation with checks for multiple borrows. 
/// Here this version removes the safeguards assuming users handle the rest.
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
        SharedSingleton { owner: Cell::new(Owner::Producer), ucell: Self::INIT_U  }
    }

    #[inline]
    pub fn is_producer_owned(&self) -> bool {
        self.owner.get() == Owner::Producer
    }
    #[inline]
    pub fn is_consumer_owned(&self) -> bool {
        self.owner.get() == Owner::Consumer
    }


    /// Returns mutable reference of T if singleton is owned by the producer
    /// NOTE: does not check for multiple mutable calls!
    #[inline]
    pub fn get_mut_ref(&self) -> Option<&mut T> {
        if self.is_producer_owned() {
            let x: *mut MaybeUninit<T> = self.ucell.get();
            let t: &mut T = unsafe {  &mut *(x as *mut T)};
            Some(t)
        }
        else {
            None
        }
    }

    /// Pass ownership to Consumer from Producer
    #[inline]
    pub fn pass_to_consumer(&self) -> Result<(),ErrCode> {
        if self.is_producer_owned() {
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
    pub fn get_ref(&self) -> Option<&T> {
        if self.is_consumer_owned() {
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
    pub fn return_to_producer(&self) -> Result<(),ErrCode> {
        if self.is_consumer_owned() {
            self.owner.set(Owner::Producer);
            Ok(())
        }
        else {
            Err(ErrCode::NotOwned)
        }
    }
}
