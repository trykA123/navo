// Navo's answer to Tracy's custom user-info hook, so that a trace header names no person and
// no machine: it always reads navo@tester (SC14).
//
// This file is never #include-d. It is force-included (-FI<path> on MSVC, -include <path> on
// gcc and clang) as the very first thing in the client's single translation unit, which is why
// the macro must be defined here and why the three functions are plain globals: every call
// site looks them up unqualified from inside namespace tracy, and the lookup reaches the global
// scope only when the definitions already precede the includes.
#pragma once

#include <cstddef>
#include <cstring>

#define TRACY_HAS_CUSTOM_USER_INFO 1

/// The login part of the header, fixed, so no real account name is ever stored.
static inline const char* PlatformGetUserLogin() { return "navo"; }

/// The full name, absent: Tracy writes the " (full name)" part only when this is non-null.
static inline const char* PlatformGetUserFullName() { return nullptr; }

/// The machine part of the header, fixed. The caller always passes a zeroed buffer, so the
/// copy keeps its own terminator and "tester" is all that is ever read back.
static inline void PlatformGetHostname(char* buf, size_t size)
{
    strncpy(buf, "tester", size - 1);
}
