pub use windows::Win32::Foundation::*;
pub use windows::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
pub use windows::Win32::Graphics::Dwm::*;
// Explicit imports instead of a glob: several Gdi names collide with
// WindowsAndMessaging and would become ambiguous at the use site.
pub use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateDIBSection, CreatePen, CreateSolidBrush, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, EndPaint, EnumDisplayMonitors, FillRect, GetDC, GetPixel,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, FF_MODERN,
    FIXED_PITCH, FW_SEMIBOLD, GetStockObject, HBITMAP, HDC, HGDIOBJ, HMONITOR, HRGN,
    InvalidateRect, MONITOR_DEFAULTTONEAREST, MonitorFromWindow, NULL_BRUSH,
    OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, PS_SOLID, Rectangle, ReleaseDC, SRCCOPY, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT, TextOutW, UpdateWindow,
};
pub use windows::Win32::System::DataExchange::*;
pub use windows::Win32::System::Memory::*;
pub use windows::Win32::Media::*;
pub use windows::Win32::Security::*;
pub use windows::Win32::System::Com::*;
pub use windows::Win32::System::LibraryLoader::*;
pub use windows::Win32::System::Power::*;
pub use windows::Win32::System::Registry::*;
pub use windows::Win32::System::Shutdown::*;
pub use windows::Win32::System::Threading::*;
pub use windows::Win32::UI::HiDpi::*;
pub use windows::Win32::UI::Input::KeyboardAndMouse::*;
pub use windows::Win32::UI::Shell::*;
pub use windows::Win32::UI::WindowsAndMessaging::*;
// BOOL moved out of Win32::Foundation into windows-core in the 0.62 family;
// the EnumWindows callback has to return exactly that type.
pub use windows::Win32::System::Diagnostics::ToolHelp::*;
pub use windows::core::{BOOL, PCWSTR, PWSTR, s, w};
