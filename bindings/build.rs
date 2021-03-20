fn main() {
    windows::build!(
        windows::win32::direct3d12::*,
        windows::win32::direct3d_hlsl::*,
        windows::win32::dxgi::*,
        windows::win32::display_devices::RECT,
        windows::win32::gdi::ValidateRect,
        windows::win32::menus_and_resources::{HMENU, HICON},
        windows::win32::windows_and_messaging::{
            CreateWindowExA, DefWindowProcA, DispatchMessageA, GetMessageA, PostQuitMessage,
            TranslateMessage,
            RegisterClassA, LoadCursorA, HWND, LPARAM, MSG, WNDCLASSA, WPARAM,
            IDC_ARROW, WM_CREATE,
            WM_DESTROY, WM_PAINT, WINDOWS_STYLE, WINDOWS_EX_STYLE, WNDCLASS_STYLES
        },
        windows::win32::system_services::{
            GetModuleHandleA, HINSTANCE, LRESULT, CreateEventA, WaitForSingleObject, WaitForSingleObjectEx
        },
        windows::win32::direct_composition::{IDCompositionDevice, IDCompositionTarget, IDCompositionVisual, DCompositionCreateDevice}
    );
}

/*
fn main() {
    windows::build!(
        // windows::win32::direct3d11::*,
        // windows::win32::dxgi::*,
        windows::win32::gdi::*,
        windows::win32::windows_and_messaging::*,
        windows::win32::windows_programming::*,
        windows::win32::system_services::*,
        windows::win32::menus_and_resources::*
    );
}
*/
