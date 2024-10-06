#include <windows.h>
#include <iostream>
#include "../include/resource.h"

LRESULT CALLBACK WindowProcedure(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    switch (msg)
    {
        std::cout << "Got message " << msg << std::endl;
    case WM_COMMAND:
        switch (LOWORD(wp))
        {
        case 1: // Plus button
            MessageBoxW(hwnd, L"Plus Button Clicked", L"Info", MB_OK);
            break;
        case 2: // Minus button
            MessageBoxW(hwnd, L"Minus Button Clicked", L"Info", MB_OK);
            break;
        case 3: // Button 1
            MessageBoxW(hwnd, L"Button 1 Clicked", L"Info", MB_OK);
            break;
        case 4: // Button 2
            MessageBoxW(hwnd, L"Button 2 Clicked", L"Info", MB_OK);
            break;
        case 5: // Button 3
            MessageBoxW(hwnd, L"Button 3 Clicked", L"Info", MB_OK);
            break;
        case 6: // Button 4
            MessageBoxW(hwnd, L"Button 4 Clicked", L"Info", MB_OK);
            break;
        }
        break;
    case WM_DESTROY:
        PostQuitMessage(0);
        break;
    default:
        return DefWindowProc(hwnd, msg, wp, lp);
    }
    return 0;
}

int WINAPI WinMain(HINSTANCE hInst, HINSTANCE hPrevInst, LPSTR args,
                   int ncmdshow)
{
    WNDCLASSW wc = {0};
    wc.hbrBackground = (HBRUSH)COLOR_WINDOW;
    wc.hCursor = LoadCursor(NULL, IDC_ARROW);
    wc.hInstance = hInst;
    wc.lpszClassName = L"myWindowClass";
    wc.lpfnWndProc = WindowProcedure;
    wc.hIcon = LoadIcon(GetModuleHandle(NULL), MAKEINTRESOURCE(IDI_MYICON));

    if (!RegisterClassW(&wc))
        return -1;

    HWND hwnd = CreateWindowW(L"myWindowClass", L"Hello, Windows!",
                              WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 500, 500, NULL,
                              NULL, hInst, NULL);

    // Create buttons
    CreateWindowW(L"BUTTON", L"+", WS_VISIBLE | WS_CHILD, 50, 50, 50, 50, hwnd, (HMENU)1, hInst, NULL);
    CreateWindowW(L"BUTTON", L"-", WS_VISIBLE | WS_CHILD, 120, 50, 50, 50, hwnd, (HMENU)1, hInst, NULL);
    CreateWindowW(L"BUTTON", L"1", WS_VISIBLE | WS_CHILD, 50, 120, 50, 50, hwnd, (HMENU)1, hInst, NULL);
    CreateWindowW(L"BUTTON", L"2", WS_VISIBLE | WS_CHILD, 120, 120, 50, 50, hwnd, (HMENU)1, hInst, NULL);
    CreateWindowW(L"BUTTON", L"3", WS_VISIBLE | WS_CHILD, 190, 120, 50, 50, hwnd, (HMENU)1, hInst, NULL);
    CreateWindowW(L"BUTTON", L"4", WS_VISIBLE | WS_CHILD, 260, 120, 50, 50, hwnd, (HMENU)1, hInst, NULL);

    MSG msg = {0};

    while (GetMessage(&msg, NULL, 0, 0))
    {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }
    return 0;
}
