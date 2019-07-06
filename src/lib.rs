/*!
A derive proc macro allowing to locks simultaneously severals locks (based on futures) and prevents
deadlocks by always sorting the locks in the same order.

# Example

```
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
```
!*/

extern crate proc_macro;
extern crate proc_macro2;

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Result;
use syn::{parse_macro_input, Data, DeriveInput, Error, Field, Fields, Ident, Type};

#[proc_macro_derive(Locks)]
pub fn locks(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);

    let s = match Struct::from_derive_input(derive_input) {
        Ok(s) => s,
        Err(e) => {
            return e.to_compile_error().into();
        }
    };

    let name = s.name.clone();
    let impl_resolve = s.impl_resolve();
    let impl_traits = s.impl_traits();

    quote! (
        impl #name {
            #impl_resolve
        }

        #impl_traits
    )
    .into()
}

struct LockField {
    idx: usize,
    name: Ident,
    ty: Type,
}

impl LockField {
    fn new_from_field(f: &Field, idx: usize) -> Result<Self> {
        let name = f
            .ident
            .clone()
            .expect("lock field must have a name.")
            .clone();

        let ty = f.ty.clone();

        Ok(LockField { idx, name, ty })
    }

    fn arg_name(&self) -> Ident {
        Ident::new(&format!("__a{}", self.idx), self.name.span())
    }

    fn impl_resolve(&self, inner_code: TokenStream) -> TokenStream {
        let name = self.name.clone();
        let arg = self.arg_name();
        let ty = self.ty.clone();

        quote! { #name!(resolve #ty).and_then(move |#arg| #inner_code) }
    }

    fn impl_traits(&self, struct_name: Ident) -> TokenStream {
        let name = self.name.clone();
        let ty = self.ty.clone();

        quote! { #name! { traits #ty, #struct_name } }
    }
}

struct Struct {
    fields: Vec<LockField>,
    name: Ident,
}

impl Struct {
    fn from_derive_input(input: DeriveInput) -> Result<Self> {
        let name = input.ident;

        let data_struct = match input.data {
            Data::Struct(s) => s,
            _ => {
                return Err(Error::new(
                    name.span(),
                    "Only struct are supported by lock derive.",
                ))
            }
        };

        let mut fields = Vec::new();

        match data_struct.fields {
            Fields::Named(f) => {
                for (idx, f) in f.named.iter().enumerate() {
                    fields.push(LockField::new_from_field(f, idx)?);
                }
            }
            _ => {
                return Err(Error::new(
                    name.span(),
                    "Only struct with named fields are supported.",
                ))
            }
        };

        fields.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Struct { fields, name })
    }

    fn impl_instantiate(&self) -> TokenStream {
        let name = self.name.clone();
        let names = self.fields.iter().map(|f| &f.name).cloned();
        let args = self.fields.iter().map(|f| f.arg_name());

        quote! {
            futures::future::ok(#name {
                #(#names: #args,)*
            })
        }
    }

    fn impl_resolve(&self) -> TokenStream {
        let name = self.name.clone();
        let mut inner_code = Some(self.impl_instantiate());

        for field in &self.fields {
            inner_code = Some(field.impl_resolve(inner_code.take().expect("inner_code")));
        }

        let inner_code = inner_code.expect("inner_code");

        quote! {
            fn resolve() -> impl futures::Future<Item = #name, Error = failure::Error> {
                use futures::future::Future;

                #inner_code
            }
        }
    }

    fn impl_traits(&self) -> TokenStream {
        let traits = self.fields.iter().map(|f| f.impl_traits(self.name.clone()));

        quote! { #(#traits)* }
    }
}
