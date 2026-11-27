/* Canonical FFI layout for the panechord macOS bridge.
 *
 * This header is the single source of truth for parameter-block layouts
 * shared by Rust (`macos/src/lib.rs`, `#[repr(C)]` mirrors) and Swift
 * (`macos/native/AppKit.swift`, imported via `w3_bridging.h` with
 * `swiftc -import-objc-header`, so the Swift structs below have exactly
 * this layout). Field order is part of the ABI: doubles first, then
 * 32-bit integers, so neither side needs padding. Any field change must
 * update the Rust mirrors, the Swift call sites, and the size assertions
 * on both sides together.
 */
#ifndef PANECHORD_W3_TYPES_H
#define PANECHORD_W3_TYPES_H

#include <stddef.h>
#include <stdint.h>

/* Grid/Absolute placement for one view id. The id itself stays a
 * separate (bytes, len) pair; only the numeric payload is packed. */
typedef struct W3PlaceParams {
    int32_t row;
    int32_t column;
    int32_t row_span;
    int32_t column_span;
    double x;
    double y;
} W3PlaceParams;

/* Full style payload for one view id. `text_scale` rides with the
 * doubles so the tail of 32-bit fields stays densely packed. */
typedef struct W3StyleParams {
    double top;
    double right;
    double bottom;
    double left;
    double width;
    double height;
    double text_scale;
    int32_t horizontal;
    int32_t vertical;
    int32_t text_role;
    int32_t red;
    int32_t green;
    int32_t blue;
} W3StyleParams;

#ifdef __cplusplus
static_assert(sizeof(W3PlaceParams) == 32, "W3PlaceParams layout changed");
static_assert(sizeof(W3StyleParams) == 80, "W3StyleParams layout changed");
static_assert(alignof(W3PlaceParams) == 8, "W3PlaceParams alignment changed");
static_assert(alignof(W3StyleParams) == 8, "W3StyleParams alignment changed");
#else
_Static_assert(sizeof(W3PlaceParams) == 32, "W3PlaceParams layout changed");
_Static_assert(sizeof(W3StyleParams) == 80, "W3StyleParams layout changed");
_Static_assert(_Alignof(W3PlaceParams) == 8, "W3PlaceParams alignment changed");
_Static_assert(_Alignof(W3StyleParams) == 8, "W3StyleParams alignment changed");
#endif

#endif /* PANECHORD_W3_TYPES_H */
