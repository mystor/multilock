#![no_std]
//! Lock and acquire references to multiple objects without deadlocks.
//!
//! This crate provides the [`multilock`] method, and related objects, which
//! allow a series of mutexes to be acquired without deadlocks.
//!
//! The passed-in closure is called with a [`Builder`], which can have a
//! series of mutexes registered with it using the [`Builder::add`]
//! method. Locks will be acquired when [`Builder::finish`] is called, in
//! ascending address order.
//!
//! # Example
//!
//! ```
//! # use multilock::multilock;
//! let m1 = parking_lot::Mutex::new(5);
//! let m2 = parking_lot::Mutex::new("cheese");
//!
//! multilock(|mut builder| {
//!     // Register the mutexes we want to lock with the builder parameter.
//!     // The returned `Token` will be used to access the locked value.
//!     let mut m1_token = builder.add(&m1);
//!     let mut m2_token = builder.add(&m2);
//!
//!     // Call `finish` once all mutexes have been registered, locking all
//!     // registered mutexes.
//!     let locker = builder.finish();
//!
//!     // Data within the locked mutexes may be accessed using the `get` and
//!     // `get_mut` methods on the token objects.
//!     assert_eq!(*m1_token.get(&locker), 5);
//!     assert_eq!(*m2_token.get(&locker), "cheese");
//!     *m1_token.get_mut(&locker) = 10;
//!     *m2_token.get_mut(&locker) = "pies";
//! });
//!
//! assert_eq!(*m1.lock(), 10);
//! assert_eq!(*m2.lock(), "pies");
//! ```

use core::marker::PhantomData;
use lock_api::{Mutex, RawMutex};
use smallvec::SmallVec;

// Invariant marker lifetime helper.
type Id<'id> = PhantomData<&'id mut &'id u8>;

/// Reference a mutex which was registered with a `Locker`.
///
/// When combined with a `Locker`, may be used to access the locked data.
pub struct Token<'id, 'a, R: RawMutex, T> {
    mutex: &'a Mutex<R, T>,
    marker: PhantomData<(Id<'id>, &'a mut T, R::GuardMarker)>,
}

impl<'id, 'a, R: RawMutex, T> Token<'id, 'a, R, T> {
    /// Get a shared reference to the value locked with this token.
    pub fn get<'b>(&'b self, _locker: &'b Locker<'id, 'a, R>) -> &'b T {
        debug_assert!(self.mutex.is_locked());
        // safety: The invariant 'id lifetime ensures that this `Token` is
        // derived from the same `Builder` as the `Locker` argument, meaning the
        // lock is currently being held.
        unsafe { &*self.mutex.data_ptr() }
    }

    /// Get a mutable reference to the value locked with this token.
    pub fn get_mut<'b>(&'b mut self, _locker: &'b Locker<'id, 'a, R>) -> &'b mut T {
        debug_assert!(self.mutex.is_locked());
        // safety: The invariant 'id lifetime ensures that this `Token` is
        // derived from the same `Builder` as the `Locker` argument, meaning the
        // lock is currently being held.
        unsafe { &mut *self.mutex.data_ptr() }
    }
}

/// Builder type used to register `Mutex` references to be locked.
///
/// Created using the `multilock` method.
pub struct Builder<'id, 'a, R: RawMutex> {
    locks: SmallVec<[&'a R; 4]>,
    marker: PhantomData<(Id<'id>, R::GuardMarker)>,
}

impl<'id, 'a, R: RawMutex> Builder<'id, 'a, R> {
    /// Register a new mutex be locked by this `Builder`.
    pub fn add<T>(&mut self, mutex: &'a Mutex<R, T>) -> Token<'id, 'a, R, T> {
        // Safety: Acquiring a reference to lock and unlock the underlying raw
        // mutex in other methods.
        unsafe {
            self.locks.push(mutex.raw());
        }
        Token {
            mutex,
            marker: PhantomData,
        }
    }

    /// Lock all mutexes registered with this builder, producing a `Locker` which
    /// will allow access to the locked data.
    pub fn finish(self) -> Locker<'id, 'a, R> {
        // Acquire each lock in our internal `Vec` in address order, which
        // should avoid deadlock issues if sets of mutexes are always locked
        // with this helper type.
        let mut locks = self.locks;
        locks.sort_unstable_by_key(|m| *m as *const R);
        for raw in &locks {
            raw.lock();
        }
        Locker {
            locks,
            marker: PhantomData,
        }
    }
}

/// Guard object representing a set of locked mutexes.
///
/// Created using the `Builder::finish` method.
#[must_use = "if unused, the Mutexes will immediately unlock"]
pub struct Locker<'id, 'a, R: RawMutex> {
    locks: SmallVec<[&'a R; 4]>,
    marker: PhantomData<(Id<'id>, R::GuardMarker)>,
}

impl<'id, 'a, R: RawMutex> Drop for Locker<'id, 'a, R> {
    fn drop(&mut self) {
        for raw in &self.locks {
            // safety: These locks were locked by `LockBuilder::finish()` when
            // this `Locker` was constructed.
            unsafe {
                raw.unlock();
            }
        }
    }
}

/// Lock and acquire references to multiple objects without deadlocks.
///
/// See the module-level documentation for details.
pub fn multilock<'a, R: RawMutex + 'a, F, O>(func: F) -> O
where
    F: for<'id> FnOnce(Builder<'id, 'a, R>) -> O,
{
    func(Builder {
        locks: SmallVec::new(),
        marker: PhantomData,
    })
}
