use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSString, NSUserDefaults, ns_string};

pub mod locale;
pub mod proc;

pub fn disable_autofill() {
    unsafe {
        NSUserDefaults::standardUserDefaults().registerDefaults(
            &NSDictionary::<NSString, AnyObject>::from_slices(
                &[ns_string!("NSAutoFillHeuristicControllerEnabled")],
                &[ns_string!("NO")],
            ),
        );
    }
}
