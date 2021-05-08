fn main() {
    windows::build!(
        Windows::Win32::Graphics::Direct3D11::{ID3DBlob},
        Windows::Win32::Graphics::Direct3D12::*,
        Windows::Win32::Graphics::Hlsl::*,
        Windows::Win32::Graphics::Dxgi::*,
        Windows::Win32::UI::DisplayDevices::{RECT},
        Windows::Win32::UI::HiDpi::{SetProcessDpiAwareness, PROCESS_DPI_AWARENESS},
        Windows::Win32::Graphics::Gdi::{ValidateRect, ClientToScreen},
        Windows::Win32::UI::MenusAndResources::{HMENU, HICON},
        Windows::Win32::UI::KeyboardAndMouseInput::{
            SetCapture, ReleaseCapture
        },
        Windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExA, DefWindowProcA, DispatchMessageA, GetMessageA, PostQuitMessage, PeekMessageA,
            TranslateMessage,
            RegisterClassA, LoadCursorW, ShowCursor, SetCursor, SetCursorPos, ClipCursor, HWND, LPARAM, MSG, WNDCLASSA, WPARAM,
            IDC_ARROW, IDC_HAND, IDC_SIZEALL, WM_CREATE, CW_USEDEFAULT,
            WM_DESTROY, WM_PAINT, WM_QUIT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WINDOW_EX_STYLE, WNDCLASS_STYLES, PEEK_MESSAGE_REMOVE_TYPE
        },
        Windows::Win32::System::SystemServices::{
            GetModuleHandleA, HINSTANCE, LRESULT
        },
        Windows::Win32::System::Threading::{
            CreateEventA, WaitForSingleObject, WaitForSingleObjectEx
        },
        Windows::Win32::Graphics::DirectComposition::{IDCompositionDevice, IDCompositionTarget, IDCompositionVisual, DCompositionCreateDevice}
    );
}

/*
fn main() {
    Windows::build!(
        // Windows::Win32::direct3d11::*,
        // Windows::Win32::dxgi::*,
        Windows::Win32::gdi::*,
        Windows::Win32::Windows_and_messaging::*,
        Windows::Win32::Windows_programming::*,
        Windows::Win32::system_services::*,
        Windows::Win32::menus_and_resources::*
    );
}
*/
