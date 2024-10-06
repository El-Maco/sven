#include <windows.h>

LRESULT CALLBACK WindowProcedure(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
  switch (msg) {
  case WM_DESTROY:
    PostQuitMessage(0);
    break;
  default:
    return DefWindowProc(hwnd, msg, wp, lp);
  }
  return 0;
}

int WINAPI WinMain(HINSTANCE hInst, HINSTANCE hPrevInst, LPSTR args,
                   int ncmdshow) {
  WNDCLASSW wc = {0};
  wc.hbrBackground = (HBRUSH)COLOR_WINDOW;
  wc.hCursor = LoadCursor(NULL, IDC_ARROW);
  wc.hInstance = hInst;
  wc.lpszClassName = L"myWindowClass";
  wc.lpfnWndProc = WindowProcedure;

  if (!RegisterClassW(&wc))
    return -1;

  CreateWindowW(L"myWindowClass", L"Hello, Windows!",
                WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 500, 500, NULL,
                NULL, hInst, NULL);

  MSG msg = {0};

  while (GetMessage(&msg, NULL, NULL, NULL)) {
    TranslateMessage(&msg);
    DispatchMessage(&msg);
  }
}
