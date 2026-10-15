//! The look's names, borrowed from the application (ui.md, section 8).

use crate::{Owner, Region, Style};

/// One owner's prefix, style token names and region names.
#[derive(Clone, Copy, Debug)]
pub struct Names<'a> {
    pub owner: Owner,
    pub prefix: &'a [u8],
    /// Indexed by `Style.token`.
    pub styles: &'a [&'a [u8]],
    /// Indexed by `Region.number`.
    pub regions: &'a [&'a [u8]],
}

/// The first fault; `index` counts the owner's styles, then its regions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fault {
    /// A prefix is empty or contains bytes outside lowercase letters and digits.
    Prefix { owner: Owner },
    /// An earlier owner has this prefix.
    SamePrefix { owner: Owner },
    /// A name lacks its prefix and hyphen, or has an invalid suffix byte.
    OffPrefix { owner: Owner, index: u32 },
    /// An earlier style or region has this name.
    Twice { owner: Owner, index: u32 },
}

/// Check prefixes, then names, giving the first fault in each class.
#[must_use]
pub fn check(names: &[Names<'_>]) -> Option<Fault> {
    for names in names {
        if names.prefix.is_empty() {
            return Some(Fault::Prefix { owner: names.owner });
        }
        for byte in names.prefix {
            if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() {
                return Some(Fault::Prefix { owner: names.owner });
            }
        }
    }
    for (position, owner) in names.iter().enumerate() {
        for previous in names.iter().take(position) {
            if previous.prefix == owner.prefix {
                return Some(Fault::SamePrefix { owner: owner.owner });
            }
        }
    }
    for owner in names {
        for (index, name) in owner.styles.iter().chain(owner.regions).enumerate() {
            let index = u32::try_from(index).expect("look table length fits u32");
            let suffix = name.strip_prefix(owner.prefix);
            let suffix = match suffix {
                Some(suffix) => suffix.strip_prefix(b"-"),
                None => None,
            };
            let Some(suffix) = suffix else {
                return Some(Fault::OffPrefix { owner: owner.owner, index });
            };
            for byte in suffix {
                if !byte.is_ascii_alphanumeric() && *byte != b'-' {
                    return Some(Fault::OffPrefix { owner: owner.owner, index });
                }
            }
        }
    }
    for (position, owner) in names.iter().enumerate() {
        for (index, name) in owner.styles.iter().chain(owner.regions).enumerate() {
            for (previous_position, previous_owner) in
                names.iter().enumerate().take(position.checked_add(1).expect("owner position fits"))
            {
                let limit = if previous_position == position { index } else { usize::MAX };
                for previous in previous_owner.styles.iter().chain(previous_owner.regions).take(limit) {
                    if previous == name {
                        let index = u32::try_from(index).expect("look table length fits u32");
                        return Some(Fault::Twice { owner: owner.owner, index });
                    }
                }
            }
        }
    }
    None
}

impl Names<'_> {
    /// Find a style name belonging to this owner, by its token.
    #[must_use]
    pub fn style(&self, style: Style) -> Option<&[u8]> {
        if style.owner != self.owner {
            return None;
        }
        self.styles.get(usize::from(style.token)).copied()
    }

    /// Find a region name belonging to this owner, by its number.
    #[must_use]
    pub fn region(&self, region: Region) -> Option<&[u8]> {
        if region.owner != self.owner {
            return None;
        }
        self.regions.get(usize::from(region.number)).copied()
    }
}
