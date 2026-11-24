//! View transform (UU features: screen_zoomscale / screen_orientation).
//! Pure geometry for the controller's display of the remote frame: rotate the
//! image 0/90/180/270 and apply a zoom factor, plus the exact inverse mapping
//! from a pointer position back to remote-desktop coordinates. Keeping these as
//! free functions makes the tricky rotation-inverse fully unit-testable.

/// Rotation in clockwise degrees, constrained to the four cardinal orientations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn from_deg(d: i32) -> Self {
        match ((d % 360) + 360) % 360 {
            90 => Rotation::R90,
            180 => Rotation::R180,
            270 => Rotation::R270,
            _ => Rotation::R0,
        }
    }

    pub fn deg(self) -> i32 {
        match self {
            Rotation::R0 => 0,
            Rotation::R90 => 90,
            Rotation::R180 => 180,
            Rotation::R270 => 270,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Rotation::R0 => Rotation::R90,
            Rotation::R90 => Rotation::R180,
            Rotation::R180 => Rotation::R270,
            Rotation::R270 => Rotation::R0,
        }
    }

    /// Does this rotation swap the width/height axes?
    pub fn swaps_axes(self) -> bool {
        matches!(self, Rotation::R90 | Rotation::R270)
    }
}

/// Displayed content size for a source (w,h) after rotation (before zoom).
pub fn rotated_size(w: f32, h: f32, rot: Rotation) -> (f32, f32) {
    if rot.swaps_axes() {
        (h, w)
    } else {
        (w, h)
    }
}

/// Aspect-fit the rotated content into `avail` with a zoom multiplier (>=1 zooms in).
/// Returns the drawn rectangle size.
pub fn fit_size(avail_w: f32, avail_h: f32, src_w: f32, src_h: f32, rot: Rotation, zoom: f32) -> (f32, f32) {
    let (rw, rh) = rotated_size(src_w, src_h, rot);
    let zoom = zoom.max(1.0);
    // base fit into available, then zoom expands beyond the fit (may overflow)
    let scale = (avail_w / rw.max(1.0)).min(avail_h / rh.max(1.0));
    let s = scale * zoom;
    ((rw * s).max(1.0), (rh * s).max(1.0))
}

/// Map a pointer position inside the drawn rect back to remote source coords,
/// inverting both the zoom-fit scale and the rotation. `rect` is where the
/// rotated+zoomed image was drawn (its own w/h already reflect rotation+zoom).
pub fn pointer_to_remote(
    px: f32,
    py: f32,
    rect_min_x: f32,
    rect_min_y: f32,
    rect_w: f32,
    rect_h: f32,
    src_w: f32,
    src_h: f32,
    rot: Rotation,
) -> (f32, f32) {
    if rect_w <= 1.0 || rect_h <= 1.0 || src_w <= 0.0 || src_h <= 0.0 {
        return (0.0, 0.0);
    }
    // normalized position within the drawn (already-rotated) image, 0..1
    let nx = ((px - rect_min_x) / rect_w).clamp(0.0, 1.0);
    let ny = ((py - rect_min_y) / rect_h).clamp(0.0, 1.0);
    // inverse-rotate normalized coords back into source orientation.
    // forward (source u,v -> display nx,ny): R0=(u,v) R90=(1-v,u) R180=(1-u,1-v) R270=(v,1-u)
    // so the exact inverses are:
    let (sx, sy) = match rot {
        Rotation::R0 => (nx, ny),
        Rotation::R90 => (ny, 1.0 - nx),
        Rotation::R180 => (1.0 - nx, 1.0 - ny),
        Rotation::R270 => (1.0 - ny, nx),
    };
    (sx * src_w, sy * src_h)
}

/// Forward-map a source coord (sx,sy in source pixels) to a point inside the drawn
/// rect. Exact inverse of `pointer_to_remote`; used to place the remote cursor overlay.
pub fn remote_to_pointer(
    sx: f32,
    sy: f32,
    rect_min_x: f32,
    rect_min_y: f32,
    rect_w: f32,
    rect_h: f32,
    src_w: f32,
    src_h: f32,
    rot: Rotation,
) -> (f32, f32) {
    if src_w <= 0.0 || src_h <= 0.0 {
        return (rect_min_x, rect_min_y);
    }
    let (u, v) = (sx / src_w, sy / src_h);
    let (nx, ny) = match rot {
        Rotation::R0 => (u, v),
        Rotation::R90 => (1.0 - v, u),
        Rotation::R180 => (1.0 - u, 1.0 - v),
        Rotation::R270 => (v, 1.0 - u),
    };
    (rect_min_x + nx * rect_w, rect_min_y + ny * rect_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_roundtrip_deg() {
        assert_eq!(Rotation::from_deg(90), Rotation::R90);
        assert_eq!(Rotation::from_deg(-90), Rotation::R270);
        assert_eq!(Rotation::from_deg(450), Rotation::R90);
        assert_eq!(Rotation::R270.next(), Rotation::R0);
    }

    #[test]
    fn rotated_size_swaps() {
        assert_eq!(rotated_size(100.0, 40.0, Rotation::R0), (100.0, 40.0));
        assert_eq!(rotated_size(100.0, 40.0, Rotation::R90), (40.0, 100.0));
        assert_eq!(rotated_size(100.0, 40.0, Rotation::R270), (40.0, 100.0));
    }

    #[test]
    fn pointer_identity_no_rotation() {
        // rect == source aspect, rot 0 => corners map to corners
        let (x, y) = pointer_to_remote(0.0, 0.0, 0.0, 0.0, 100.0, 50.0, 100.0, 50.0, Rotation::R0);
        assert_eq!((x, y), (0.0, 0.0));
        let (x, y) = pointer_to_remote(100.0, 50.0, 0.0, 0.0, 100.0, 50.0, 100.0, 50.0, Rotation::R0);
        assert_eq!((x, y), (100.0, 50.0));
    }

    #[test]
    fn pointer_rot90_maps_corners() {
        // source 100x40 rotated 90 CW => drawn 40x100.
        // forward R90: (u,v)->(1-v,u). source TL(0,0)->display(1,0)=px(40,0).
        let (x, y) = pointer_to_remote(40.0, 0.0, 0.0, 0.0, 40.0, 100.0, 100.0, 40.0, Rotation::R90);
        assert_eq!((x, y), (0.0, 0.0));
        // source BL(0,40): u=0,v=1 -> display(0,0)=px(0,0).
        let (x, y) = pointer_to_remote(0.0, 0.0, 0.0, 0.0, 40.0, 100.0, 100.0, 40.0, Rotation::R90);
        assert_eq!((x, y), (0.0, 40.0));
    }

    #[test]
    fn pointer_rot180_flips() {
        let (x, y) = pointer_to_remote(0.0, 0.0, 0.0, 0.0, 100.0, 50.0, 100.0, 50.0, Rotation::R180);
        assert_eq!((x, y), (100.0, 50.0));
    }

    #[test]
    fn pointer_rot270_maps_corners() {
        // source 100x40 rotated 270 CW => drawn 40x100. inverse R270: (sx,sy)=(1-ny,nx).
        // display BL px(0,100): nx=0,ny=1 -> sx=0,sy=0 -> source TL (0,0).
        let (x, y) = pointer_to_remote(0.0, 100.0, 0.0, 0.0, 40.0, 100.0, 100.0, 40.0, Rotation::R270);
        assert_eq!((x, y), (0.0, 0.0));
        // display TR px(40,0): nx=1,ny=0 -> sx=1,sy=1 -> source BR (100,40).
        let (x, y) = pointer_to_remote(40.0, 0.0, 0.0, 0.0, 40.0, 100.0, 100.0, 40.0, Rotation::R270);
        assert_eq!((x, y), (100.0, 40.0));
    }

    #[test]
    fn forward_inverse_roundtrip_all_rotations() {
        // remote_to_pointer(pointer_to_remote(p)) == p, and vice versa, for every rotation.
        let (sw, sh) = (100.0f32, 40.0f32);
        for rot in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            let (dw, dh) = rotated_size(sw, sh, rot);
            // sample points strictly inside the drawn rect (clamp semantics make
            // out-of-rect points intentionally non-invertible)
            for (px, py) in [(10.0, 5.0), (30.0, 20.0), (0.0, 0.0), (dw, dh)] {
                let (sx, sy) = pointer_to_remote(px, py, 0.0, 0.0, dw, dh, sw, sh, rot);
                let (bx, by) = remote_to_pointer(sx, sy, 0.0, 0.0, dw, dh, sw, sh, rot);
                assert!((bx - px).abs() < 1e-3 && (by - py).abs() < 1e-3, "rot {rot:?} roundtrip ({px},{py})->({bx},{by})");
            }
        }
    }

    #[test]
    fn pointer_inverse_of_forward_all_rotations() {
        // for each rotation, forward-project source corners through the same math
        // the display side would use, then inverse must recover them.
        let (sw, sh) = (100.0f32, 40.0f32);
        let corners = [(0.0, 0.0), (sw, 0.0), (0.0, sh), (sw, sh)];
        for rot in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            let (dw, dh) = rotated_size(sw, sh, rot);
            for (cx, cy) in corners {
                // forward: source -> display (inverse of pointer_to_remote)
                let (nx, ny) = match rot {
                    Rotation::R0 => (cx / sw, cy / sh),
                    Rotation::R90 => (1.0 - cy / sh, cx / sw),
                    Rotation::R180 => (1.0 - cx / sw, 1.0 - cy / sh),
                    Rotation::R270 => (cy / sh, 1.0 - cx / sw),
                };
                let px = nx * dw;
                let py = ny * dh;
                let (bx, by) = pointer_to_remote(px, py, 0.0, 0.0, dw, dh, sw, sh, rot);
                let (ex, ey) = (bx - cx, by - cy);
                assert!(ex.abs() < 1e-4 && ey.abs() < 1e-4, "rot {rot:?} corner ({cx},{cy}) -> ({bx},{by})");
            }
        }
    }

    #[test]
    fn fit_respects_zoom_floor_and_aspect() {
        // avail 1000x1000, src 200x100, rot0, zoom1 => fit scale=5 => 1000x500
        let (w, h) = fit_size(1000.0, 1000.0, 200.0, 100.0, Rotation::R0, 1.0);
        assert_eq!((w, h), (1000.0, 500.0));
        // zoom 2 doubles
        let (w, h) = fit_size(1000.0, 1000.0, 200.0, 100.0, Rotation::R0, 2.0);
        assert_eq!((w, h), (2000.0, 1000.0));
        // zoom below 1 clamps to 1
        let (w, h) = fit_size(1000.0, 1000.0, 200.0, 100.0, Rotation::R0, 0.3);
        assert_eq!((w, h), (1000.0, 500.0));
        // rotated 90 swaps before fit
        let (w, h) = fit_size(1000.0, 1000.0, 200.0, 100.0, Rotation::R90, 1.0);
        assert_eq!((w, h), (500.0, 1000.0));
    }

    #[test]
    fn degenerate_inputs_safe() {
        let (x, y) = pointer_to_remote(5.0, 5.0, 0.0, 0.0, 0.0, 0.0, 100.0, 50.0, Rotation::R0);
        assert_eq!((x, y), (0.0, 0.0));
        let (w, h) = fit_size(0.0, 0.0, 0.0, 0.0, Rotation::R0, 1.0);
        assert!(w >= 1.0 && h >= 1.0);
    }
}
