use failure::format_err;
use futures_locks::{RwLock, RwLockReadGuard};
use tokio::executor::current_thread::block_on_all;

// this macro is a receipe on how to support a lock and what to implement
// for a lock on the lock struct
macro_rules! accounts {
    // Invoked by the derive to find and initiate the lock request.
    (resolve $field:ty) => {
        ACCOUNTS.read().map_err(|_| format_err!("Lock Error"))
    };

    // Invoked by the derive to implement traits on the struct based on the locks available.
    (traits $field:ty, $struct:ty) => {
        impl AsRef<i32> for $struct {
            fn as_ref(&self) -> &i32 {
                &self.accounts
            }
        }
    };
}

// a lock in a static field
lazy_static::lazy_static! {
    static ref ACCOUNTS: RwLock<i32> = RwLock::new(10);
}

#[derive(lock_derive::Locks)]
struct Locks {
    accounts: RwLockReadGuard<i32>,
}

fn main() {
    let future = Locks::resolve();
    let locks = block_on_all(future).unwrap();

    assert_eq!(10, *locks.accounts);
    assert_eq!(10, *locks.as_ref());
}
