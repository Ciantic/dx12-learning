fn main() {
    windows::build!(
        windows::win32::direct3d12::*,
        windows::win32::direct3d_hlsl::*,
        windows::win32::dxgi::*,
        windows::win32::display_devices::{RECT},
        windows::win32::hi_dpi::{SetProcessDpiAwareness, PROCESS_DPI_AWARENESS},
        windows::win32::gdi::{ValidateRect, ClientToScreen},
        windows::win32::menus_and_resources::{HMENU, HICON},
        windows::win32::keyboard_and_mouse_input::{
            SetCapture, ReleaseCapture
        },
        windows::win32::windows_and_messaging::{
            CreateWindowExA, DefWindowProcA, DispatchMessageA, GetMessageA, PostQuitMessage, PeekMessageA,
            TranslateMessage,
            RegisterClassA, LoadCursorA, ShowCursor, SetCursor, SetCursorPos, ClipCursor, HWND, LPARAM, MSG, WNDCLASSA, WPARAM,
            IDC_ARROW, IDC_HAND, IDC_SIZEALL, WM_CREATE, CW_USEDEFAULT,
            WM_DESTROY, WM_PAINT, WM_QUIT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WINDOWS_STYLE, WINDOWS_EX_STYLE, WNDCLASS_STYLES, PeekMessage_wRemoveMsg
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
