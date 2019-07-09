#![feature(proc_macro_hygiene)]

use failure::format_err;
use futures_locks::{RwLock, RwLockReadGuard};
use tokio::executor::current_thread::block_on_all;

// this macro is a recipe on how to support a lock and what to implement
// for a lock on the lock struct
macro_rules! accounts {
    (ty read) => { RwLockReadGuard<i32> };
    (resolve read) => { ACCOUNTS.read().map_err(|_| format_err!("Lock error")) };
    (traits $access:ident $struct:ty) => {
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

fn main() {
    let future = lock_derive::locks!(read: [accounts]);
    let locks = block_on_all(future).unwrap();
    assert_eq!(10, *locks.accounts);
    assert_eq!(10, *locks.as_ref());
}
