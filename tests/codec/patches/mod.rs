// M4's command generates this file; this one is written by hand to its shape.

mod context;
pub use context::Context;
mod p0001;

use skein_codec::patch::{Build, Halt, Old, Site};
use skein_codec::value::Node;

pub const P0001: &str = include_str!("0001.patch");

pub mod kept {
    pub mod every {
        pub mod v1 {
            pub mod numbers {
                use skein_codec::schema::FieldIx;
                pub const SMALL: FieldIx = FieldIx::new(0);
                pub const SHORT: FieldIx = FieldIx::new(1);
            }
            pub mod shape {
                use skein_codec::schema::VariantIx;
                pub const DOT: VariantIx = VariantIx::new(3);
            }
            pub mod every {
                use skein_codec::schema::FieldIx;
                pub const NUMBERS: FieldIx = FieldIx::new(22);
            }
        }
        pub mod v2 {
            pub mod shape {
                use skein_codec::schema::VariantIx;
                pub const BLANK: VariantIx = VariantIx::new(4);
            }
        }
    }
}

pub fn written(site: Site, old: &Old, new: &mut Build, at: Node, cx: &mut Context) -> Result<(), Halt> {
    match site.patch() {
        1 => match site.index() {
            0 => p0001::shape(old, new, at, cx),
            1 => p0001::every_score(old, new, at, cx),
            _ => Err(Halt::NoSite(site)),
        },
        _ => Err(Halt::NoSite(site)),
    }
}
