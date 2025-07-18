#![no_std]
extern crate alloc;

use core::ffi::{CStr, c_char, c_void};

use libafl_asan::{
    GuestAddr,
    allocator::{
        backend::dlmalloc::DlmallocBackend,
        frontend::{AllocatorFrontend, default::DefaultFrontend},
    },
    env::Env,
    file::linux::LinuxFileReader,
    logger::linux::LinuxLogger,
    mmap::linux::LinuxMmap,
    shadow::{
        Shadow,
        guest::{DefaultShadowLayout, GuestShadow},
    },
    symbols::{Symbols, nop::NopSymbols},
    tracking::{Tracking, guest_fast::GuestFastTracking},
};
use log::{Level, info, trace};
use spin::{Lazy, Mutex};

pub type ZasanFrontend = DefaultFrontend<
    DlmallocBackend<LinuxMmap>,
    GuestShadow<LinuxMmap, DefaultShadowLayout>,
    GuestFastTracking,
>;

pub type ZasanSyms = NopSymbols;

pub type ZasanEnv = Env<LinuxFileReader>;

const PAGE_SIZE: usize = 4096;

static FRONTEND: Lazy<Mutex<ZasanFrontend>> = Lazy::new(|| {
    let level = ZasanEnv::initialize()
        .ok()
        .and_then(|e| e.log_level())
        .unwrap_or(Level::Warn);
    LinuxLogger::initialize(level);
    info!("Zasan initializing...");
    let backend = DlmallocBackend::<LinuxMmap>::new(PAGE_SIZE);
    let shadow = GuestShadow::<LinuxMmap, DefaultShadowLayout>::new().unwrap();
    let tracking = GuestFastTracking::new().unwrap();
    let frontend = ZasanFrontend::new(
        backend,
        shadow,
        tracking,
        ZasanFrontend::DEFAULT_REDZONE_SIZE,
        ZasanFrontend::DEFAULT_QUARANTINE_SIZE,
    )
    .unwrap();
    info!("Zasan initialized.");
    Mutex::new(frontend)
});

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_load(addr: *const c_void, size: usize) {
    trace!("load - addr: {:#x}, size: {:#x}", addr as GuestAddr, size);
    if FRONTEND
        .lock()
        .shadow()
        .is_poison(addr as GuestAddr, size)
        .unwrap()
    {
        panic!("Poisoned - addr: {:p}, size: {:#x}", addr, size);
    }
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_store(addr: *const c_void, size: usize) {
    trace!("store - addr: {:#x}, size: {:#x}", addr as GuestAddr, size);
    if FRONTEND
        .lock()
        .shadow()
        .is_poison(addr as GuestAddr, size)
        .unwrap()
    {
        panic!("Poisoned - addr: {:p}, size: {:#x}", addr, size);
    }
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_alloc(len: usize, align: usize) -> *mut c_void {
    trace!("alloc - len: {:#x}, align: {:#x}", len, align);
    let ptr = FRONTEND.lock().alloc(len, align).unwrap() as *mut c_void;
    trace!(
        "alloc - len: {:#x}, align: {:#x}, ptr: {:p}",
        len, align, ptr
    );
    ptr
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_dealloc(addr: *const c_void) {
    trace!("free - addr: {:p}", addr);
    FRONTEND.lock().dealloc(addr as GuestAddr).unwrap();
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_get_size(addr: *const c_void) -> usize {
    trace!("get_size - addr: {:p}", addr);
    FRONTEND.lock().get_size(addr as GuestAddr).unwrap()
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_sym(name: *const c_char) -> *const c_void {
    unsafe { ZasanSyms::lookup_raw(name).unwrap() as *const c_void }
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_page_size() -> usize {
    PAGE_SIZE
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_unpoison(addr: *const c_void, len: usize) {
    trace!("unpoison - addr: {:p}, len: {:#x}", addr, len);
    FRONTEND
        .lock()
        .shadow_mut()
        .unpoison(addr as GuestAddr, len)
        .unwrap();
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_track(addr: *const c_void, len: usize) {
    trace!("track - addr: {:p}, len: {:#x}", addr, len);
    FRONTEND
        .lock()
        .tracking_mut()
        .track(addr as GuestAddr, len)
        .unwrap();
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_untrack(addr: *const c_void) {
    trace!("untrack - addr: {:p}", addr);
    FRONTEND
        .lock()
        .tracking_mut()
        .untrack(addr as GuestAddr)
        .unwrap();
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_panic(msg: *const c_char) -> ! {
    trace!("panic - msg: {:p}", msg);
    let msg = unsafe { CStr::from_ptr(msg as *const c_char) };
    panic!("{}", msg.to_str().unwrap());
}

#[unsafe(no_mangle)]
/// # Safety
pub unsafe extern "C" fn asan_swap(_enabled: bool) {
    /* Don't log since this function is on the logging path */
}
