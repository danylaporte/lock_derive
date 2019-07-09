/*!
A derive proc macro allowing to locks simultaneously severals locks (based on futures) and prevents
deadlocks by always sorting the locks in the same order.

# Example

```
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
```
!*/

extern crate proc_macro;
extern crate proc_macro2;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::collections::HashMap;
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{bracketed, parse_macro_input, Error, Ident, Token};

#[proc_macro]
pub fn locks(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let args = parse_macro_input!(item as Args);
    write_all(&args).into()
}

struct Args {
    items: Vec<(Ident, ReadWrite)>,
}

impl Parse for Args {
    fn parse(stream: ParseStream) -> Result<Self> {
        let mut read = None;
        let mut write = None;

        while !stream.is_empty() {
            let name: Ident = stream.parse()?;
            let _: Token![:] = stream.parse()?;
            let s = name.to_string();
            let s = s.as_str();

            let content;
            bracketed!(content in stream);

            let punctuated = <Punctuated<Ident, Token![,]>>::parse_terminated(&content)?;
            let vec = punctuated.into_iter().collect::<Vec<_>>();

            let old = match s {
                "read" => read.replace(vec),
                "write" => write.replace(vec),
                _ => return Err(Error::new(name.span(), "Expected `read` or `write`.")),
            };

            if old.is_some() {
                return Err(Error::new(
                    name.span(),
                    format!("`{}` found more than once.", s),
                ));
            }
        }

        let mut set = HashMap::new();
        let read = read
            .unwrap_or_else(Vec::new)
            .into_iter()
            .map(|r| (r, ReadWrite::Read));

        let write = write
            .unwrap_or_else(Vec::new)
            .into_iter()
            .map(|w| (w, ReadWrite::Write));

        let items = read.chain(write);

        for (ident, read_write) in items {
            let span = ident.span();

            if set.insert(ident, read_write).is_some() {
                return Err(Error::new(span, "Found multiple times."));
            }
        }

        let mut items = set.into_iter().collect::<Vec<_>>();
        items.sort_unstable_by(|a, b| a.0.cmp(&b.0));

        Ok(Self { items })
    }
}

#[derive(Clone, Copy)]
enum ReadWrite {
    Read,
    Write,
}

impl ReadWrite {
    fn ident(self) -> Ident {
        Ident::new(
            match self {
                ReadWrite::Read => "read",
                ReadWrite::Write => "write",
            },
            Span::call_site(),
        )
    }
}

fn write_resolve(args: &Args) -> TokenStream {
    let fields = args.items.iter().enumerate().map(|(i, t)| {
        let name = &t.0;
        let v = Ident::new(&format!("__v{}", i), Span::call_site());
        quote! { #name: #v }
    });

    let mut inner_code = Some(quote! { Ok(Locks { #(#fields,)* }) });

    for (i, t) in args.items.iter().enumerate() {
        let name = &t.0;
        let t = t.1.ident();
        let v = Ident::new(&format!("__v{}", i), Span::call_site());
        let code = inner_code.take().expect("inner_code");

        inner_code = Some(quote! { #name!(resolve #t).and_then(|#v| #code) });
    }

    let code = inner_code.expect("inner_code");

    quote! {
        impl Locks {
            fn resolve() -> impl futures::Future<Item = Self, Error = failure::Error> {
                use futures::Future;

                #code
            }
        }
    }
}

fn write_struct(args: &Args) -> TokenStream {
    let fields = args.items.iter().map(|t| {
        let n = &t.0;
        let ident = &t.1.ident();

        quote! { #n: #n!(ty #ident) }
    });

    quote! {
        struct Locks {
            #(#fields,)*
        }
    }
}

fn write_traits(args: &Args) -> TokenStream {
    let fields = args.items.iter().map(|t| {
        let n = &t.0;
        let ident = &t.1.ident();

        quote! { #n!{ traits #ident Locks  } }
    });

    quote! { #(#fields)* }
}

fn write_all(args: &Args) -> TokenStream {
    let locks = write_struct(args);
    let resolve = write_resolve(args);
    let traits = write_traits(args);

    quote! {{
        #locks
        #resolve
        #traits

        Locks::resolve()
    }}
}
