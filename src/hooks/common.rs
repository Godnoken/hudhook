use std::hint;
use std::sync::atomic::{AtomicBool, Ordering};

use imgui::{Context, Io, Key, Ui};
use parking_lot::MutexGuard;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{WHEEL_DELTA, WM_XBUTTONDBLCLK, XBUTTON1, *};

use super::dx11::ImguiDx11Hooks;
use super::dx12::ImguiDx12Hooks;
use super::dx9::ImguiDx9Hooks;
use super::opengl3::ImguiOpenGl3Hooks;
use super::{get_wheel_delta_wparam, hiword, loword, Hooks};

pub(crate) type WndProcType =
    unsafe extern "system" fn(hwnd: HWND, umsg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;

pub(crate) trait ImguiWindowsEventHandler {
    fn io(&self) -> &imgui::Io;
    fn io_mut(&mut self) -> &mut imgui::Io;

    fn focus(&self) -> bool;
    fn focus_mut(&mut self) -> &mut bool;

    fn wnd_proc(&self) -> WndProcType;

    fn setup_io(&mut self) {
        let mut io = ImguiWindowsEventHandler::io_mut(self);

        io.nav_active = true;
        io.nav_visible = true;

        // Initialize keys
        io[Key::Tab] = VK_TAB.0 as _;
        io[Key::LeftArrow] = VK_LEFT.0 as _;
        io[Key::RightArrow] = VK_RIGHT.0 as _;
        io[Key::UpArrow] = VK_UP.0 as _;
        io[Key::DownArrow] = VK_DOWN.0 as _;
        io[Key::PageUp] = VK_PRIOR.0 as _;
        io[Key::PageDown] = VK_NEXT.0 as _;
        io[Key::Home] = VK_HOME.0 as _;
        io[Key::End] = VK_END.0 as _;
        io[Key::Insert] = VK_INSERT.0 as _;
        io[Key::Delete] = VK_DELETE.0 as _;
        io[Key::Backspace] = VK_BACK.0 as _;
        io[Key::Space] = VK_SPACE.0 as _;
        io[Key::Enter] = VK_RETURN.0 as _;
        io[Key::Escape] = VK_ESCAPE.0 as _;
        io[Key::A] = VK_A.0 as _;
        io[Key::C] = VK_C.0 as _;
        io[Key::V] = VK_V.0 as _;
        io[Key::X] = VK_X.0 as _;
        io[Key::Y] = VK_Y.0 as _;
        io[Key::Z] = VK_Z.0 as _;
    }
}

#[must_use]
pub(crate) fn imgui_wnd_proc_impl<T>(
    hwnd: HWND,
    umsg: u32,
    WPARAM(wparam): WPARAM,
    LPARAM(lparam): LPARAM,
    mut imgui_renderer: MutexGuard<Box<impl ImguiWindowsEventHandler>>,
    imgui_render_loop: T,
) -> LRESULT
where
    T: AsRef<dyn Send + Sync + ImguiRenderLoop + 'static>,
{
    let mut io = imgui_renderer.io_mut();
    match umsg {
        state @ (WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP) if wparam < 256 => {
            fn map_vkey(wparam: u16, lparam: usize) -> VIRTUAL_KEY {
                match VIRTUAL_KEY(wparam) {
                    VK_SHIFT => unsafe {
                        match MapVirtualKeyA(
                            ((lparam & 0x00ff0000) >> 16) as u32,
                            MAPVK_VSC_TO_VK_EX,
                        ) {
                            0 => VIRTUAL_KEY(wparam),
                            i => VIRTUAL_KEY(i as _),
                        }
                    },
                    VK_CONTROL => {
                        if lparam & 0x01000000 != 0 {
                            VK_RCONTROL
                        } else {
                            VK_LCONTROL
                        }
                    },
                    VK_MENU => {
                        if lparam & 0x01000000 != 0 {
                            VK_RMENU
                        } else {
                            VK_LMENU
                        }
                    },
                    _ => VIRTUAL_KEY(wparam),
                }
            }

            let pressed = (state == WM_KEYDOWN) || (state == WM_SYSKEYDOWN);
            let key_pressed = map_vkey(wparam as _, lparam as _);
            io.keys_down[key_pressed.0 as usize] = pressed;

            // According to the winit implementation [1], it's ok to check twice, and the
            // logic isn't flawed either.
            //
            // [1] https://github.com/imgui-rs/imgui-rs/blob/b1e66d050e84dbb2120001d16ce59d15ef6b5303/imgui-winit-support/src/lib.rs#L401-L404
            match key_pressed {
                VK_CONTROL | VK_LCONTROL | VK_RCONTROL => io.key_ctrl = pressed,
                VK_SHIFT | VK_LSHIFT | VK_RSHIFT => io.key_shift = pressed,
                VK_MENU | VK_LMENU | VK_RMENU => io.key_alt = pressed,
                VK_LWIN | VK_RWIN => io.key_super = pressed,
                _ => (),
            };
        },
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            io.mouse_down[0] = true;
        },
        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => {
            io.mouse_down[1] = true;
        },
        WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => {
            io.mouse_down[2] = true;
        },
        WM_XBUTTONDOWN | WM_XBUTTONDBLCLK => {
            let btn = if hiword(wparam as _) == XBUTTON1 { 3 } else { 4 };
            io.mouse_down[btn] = true;
        },
        WM_LBUTTONUP => {
            io.mouse_down[0] = false;
        },
        WM_RBUTTONUP => {
            io.mouse_down[1] = false;
        },
        WM_MBUTTONUP => {
            io.mouse_down[2] = false;
        },
        WM_XBUTTONUP => {
            let btn = if hiword(wparam as _) == XBUTTON1 { 3 } else { 4 };
            io.mouse_down[btn] = false;
        },
        WM_MOUSEWHEEL => {
            let wheel_delta_wparam = get_wheel_delta_wparam(wparam as _);
            let wheel_delta = WHEEL_DELTA as f32;
            io.mouse_wheel += (wheel_delta_wparam as i16 as f32) / wheel_delta;
        },
        WM_MOUSEHWHEEL => {
            let wheel_delta_wparam = get_wheel_delta_wparam(wparam as _);
            let wheel_delta = WHEEL_DELTA as f32;
            io.mouse_wheel_h += (wheel_delta_wparam as i16 as f32) / wheel_delta;
        },
        WM_CHAR => io.add_input_character(wparam as u8 as char),
        WM_ACTIVATE => {
            *imgui_renderer.focus_mut() = loword(wparam as _) != WA_INACTIVE as u16;
            return LRESULT(1);
        },
        _ => {},
    };

    let wnd_proc = imgui_renderer.wnd_proc();
    let should_block_messages =
        imgui_render_loop.as_ref().should_block_messages(imgui_renderer.io());
    drop(imgui_renderer);

    if should_block_messages {
        return LRESULT(1);
    }

    unsafe { CallWindowProcW(Some(wnd_proc), hwnd, umsg, WPARAM(wparam), LPARAM(lparam)) }
}

/// Holds information useful to the render loop which can't be retrieved from
/// `imgui::Ui`.
pub struct ImguiRenderLoopFlags {
    /// Whether the hooked program's window is currently focused.
    pub focused: bool,
}

pub trait HookableBackend: Hooks {
    fn from_struct<T: ImguiRenderLoop + Send + Sync + Sized + 'static>(t: T) -> Self;
}

impl HookableBackend for ImguiDx9Hooks {
    fn from_struct<T: ImguiRenderLoop + Send + Sync + Sized + 'static>(t: T) -> Self {
        unsafe { ImguiDx9Hooks::new(t) }
    }
}

impl HookableBackend for ImguiDx11Hooks {
    fn from_struct<T: ImguiRenderLoop + Send + Sync + Sized + 'static>(t: T) -> Self {
        unsafe { ImguiDx11Hooks::new(t) }
    }
}

impl HookableBackend for ImguiDx12Hooks {
    fn from_struct<T: ImguiRenderLoop + Send + Sync + Sized + 'static>(t: T) -> Self {
        unsafe { ImguiDx12Hooks::new(t) }
    }
}

impl HookableBackend for ImguiOpenGl3Hooks {
    fn from_struct<T: ImguiRenderLoop + Send + Sync + Sized + 'static>(t: T) -> Self {
        unsafe { ImguiOpenGl3Hooks::new(t) }
    }
}

/// Implement your `imgui` rendering logic via this trait.
pub trait ImguiRenderLoop {
    /// Called once at the first occurrence of the hook. Implement this to
    /// initialize your data.
    fn initialize(&mut self, _ctx: &mut Context) {}
    /// Called every frame. Use the provided `ui` object to build your UI.
    fn render(&mut self, ui: &mut Ui, flags: &ImguiRenderLoopFlags);

    /// If this function returns true, the WndProc function will not call the
    /// procedure of the parent window.
    fn should_block_messages(&self, _io: &Io) -> bool {
        false
    }

    fn into_hook<T>(self) -> Box<T>
    where
        T: HookableBackend,
        Self: Send + Sync + Sized + 'static,
    {
        Box::<T>::new(HookableBackend::from_struct(self))
    }
}

/// Spin-loop based synchronization struct.
///
/// Call [`Fence::lock`] in a thread to indicate some operation is in progress,
/// and [`Fence::wait`] on a different thread to create a spin-loop that waits
/// for the lock to be dropped.
pub(crate) struct Fence(AtomicBool);

impl Fence {
    pub(crate) const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Create a [`FenceGuard`].
    pub(crate) fn lock(&self) -> FenceGuard<'_> {
        FenceGuard::new(self)
    }

    /// Wait in a spin-loop for the [`FenceGuard`] created by [`Fence::lock`] to
    /// be dropped.
    pub(crate) fn wait(&self) {
        while self.0.load(Ordering::SeqCst) {
            hint::spin_loop();
        }
    }
}

/// A RAII implementation of a spin-loop for a [`Fence`]. When this is dropped,
/// the wait on a [`Fence`] will terminate.
pub(crate) struct FenceGuard<'a>(&'a Fence);

impl<'a> FenceGuard<'a> {
    fn new(fence: &'a Fence) -> Self {
        fence.0.store(true, Ordering::SeqCst);
        Self(fence)
    }
}

impl<'a> Drop for FenceGuard<'a> {
    fn drop(&mut self) {
        self.0 .0.store(false, Ordering::SeqCst);
    }
}
