// #[macro_use]

// #[macro_export]
// macro_rules! tm_cl {
//     ($tm:expr, $terminal:ident,  { $($b:tt)* } ) => {
//         let tm = $tm.clone();
//         let mut tab_manager_guard = tm.read().unwrap();
//         let tab_manager: & TabManager = & *tab_manager_guard;
//         let tab = &*tab_manager.selected_tab_arc();
//         let terminal_mutex = tab.terminal.clone();
//         let mut terminal_guard = terminal_mutex.lock();
//         let mut $terminal = &mut *terminal_guard;
        
//         $($b)*
        
//         drop(tab_manager_guard);
//         drop(terminal_guard);
//     };
// }





// #[macro_export]
// macro_rules! tm_rw {
//     ($mut:ident $tab_manager:expr { $($b:tt)* } )  => {
//         let mut tab_manager_guard = rw_lock.write();
//         let mut $tab_manager = tab_manager_guard.unwrap();
//         $($b)*
//         drop(tab_manager_guard);
//     }, ($tab_manager:ident  { $($b:tt)* } ) => {
//         let tab_manager_guard = rw_lock.read();
//         let $tab_manager = tab_manager_guard.unwrap();
//         $($b)*
//         drop(tab_manager_guard);
//     }
// }



// #[macro_export]
// macro_rules! tm_m {
//     ($tm:expr, {$($b:tt)* }) => {
//         let mut tab_manager_guard = TabManager::mutex().lock();
//         let mut $tm = *tab_manager_guard;
        
//         $($b)*
        
//         drop(tab_manager_guard);
//     };
// }



// #[macro_export]
// macro_rules! tm_rw_o {
//     ($tab_manager:ident, $oper:expr, { $($b:tt)* } ) => {
//         let rw_lock = TabManager::arc_rw();
//         if $oper.eq("write") {
//             let mut tab_manager = rw_lock.write().unwrap();
//             $($b)*
//             drop(tab_manager);
//         } else {
//             let mut tab_manager = rw_lock.write().unwrap();
//             $($b)*
//             drop(tab_manager);
//         }
//     };
// }

// #[macro_export]
// macro_rules! tm_rw_o {
//     ($tab_manager:ident $oper:expr, { $($b:tt)* } ) => {
//         let rw_lock = TabManager::arc_rw();
//         if $tab_manager.eq("write") {
//             let mut tab_manager_guard = rw_lock.write();
//             let mut $tab_manager = tab_manager_guard.unwrap();
//             $($b)*
//             drop(tab_manager_guard);
//         } else {
//             let tab_manager_guard = rw_lock.read();
//             let $tab_manager = tab_manager_guard.unwrap();
//             $($b)*
//             drop(tab_manager_guard);
//         }
//     };
// }