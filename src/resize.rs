//! Pure resize-target math. No image decoding happens here.

/// A resize request. Modes are mutually exclusive (enforced by clap / config validation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResizeSpec {
    /// Width and/or height. Both = exact size; one alone = scale keeping aspect ratio.
    Dimensions {
        width: Option<u32>,
        height: Option<u32>,
    },
    /// Scale factor in percent (50.0 = half size).
    Percent(f32),
    /// Downscale so width <= value (no-op if already smaller).
    MaxWidth(u32),
    /// Downscale so height <= value (no-op if already smaller).
    MaxHeight(u32),
    /// Downscale so max(w, h) == value (no-op if already smaller).
    LongestEdge(u32),
    /// Scale so min(w, h) == value (upscale only with --allow-upscale).
    ShortestEdge(u32),
    /// Downscale so w*h <= value megapixels (no-op if already smaller).
    MaxMegapixels(f32),
}

/// Compute the target dimensions for a resize, or `None` when no resize is needed.
///
/// Unless `allow_upscale` is set, results are clamped so the image never grows:
/// scale modes become a no-op when the factor would be >= 1, and exact
/// dimensions are clamped to the original in each dimension.
pub fn compute_target(
    orig_w: u32,
    orig_h: u32,
    spec: &ResizeSpec,
    allow_upscale: bool,
) -> Option<(u32, u32)> {
    if orig_w == 0 || orig_h == 0 {
        return None;
    }
    let (ow, oh) = (orig_w as f64, orig_h as f64);

    // Shared by all proportional modes: apply factor, clamp upscale, round, never 0.
    let scale_by = |factor: f64| -> Option<(u32, u32)> {
        if !allow_upscale && factor >= 1.0 {
            return None;
        }
        let w = (ow * factor).round().max(1.0) as u32;
        let h = (oh * factor).round().max(1.0) as u32;
        if (w, h) == (orig_w, orig_h) {
            None
        } else {
            Some((w, h))
        }
    };

    match *spec {
        ResizeSpec::Dimensions { width, height } => match (width, height) {
            (Some(w), Some(h)) => {
                let (tw, th) = if allow_upscale {
                    (w, h)
                } else {
                    (w.min(orig_w), h.min(orig_h))
                };
                if (tw, th) == (orig_w, orig_h) {
                    None
                } else {
                    Some((tw.max(1), th.max(1)))
                }
            }
            (Some(w), None) => scale_by(w as f64 / ow),
            (None, Some(h)) => scale_by(h as f64 / oh),
            (None, None) => None,
        },
        ResizeSpec::Percent(p) => scale_by(p as f64 / 100.0),
        ResizeSpec::MaxWidth(m) => {
            if orig_w > m {
                scale_by(m as f64 / ow)
            } else {
                None
            }
        }
        ResizeSpec::MaxHeight(m) => {
            if orig_h > m {
                scale_by(m as f64 / oh)
            } else {
                None
            }
        }
        ResizeSpec::LongestEdge(m) => {
            let longest = orig_w.max(orig_h);
            if longest > m {
                scale_by(m as f64 / longest as f64)
            } else {
                None
            }
        }
        ResizeSpec::ShortestEdge(m) => {
            let shortest = orig_w.min(orig_h);
            if shortest == m {
                None
            } else if shortest > m || allow_upscale {
                scale_by(m as f64 / shortest as f64)
            } else {
                None
            }
        }
        ResizeSpec::MaxMegapixels(mp) => {
            let max_px = mp as f64 * 1_000_000.0;
            let px = ow * oh;
            if px > max_px {
                scale_by((max_px / px).sqrt())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 1000;
    const H: u32 = 500; // 2:1

    fn dims(width: Option<u32>, height: Option<u32>) -> ResizeSpec {
        ResizeSpec::Dimensions { width, height }
    }

    #[test]
    fn exact_dimensions() {
        assert_eq!(
            compute_target(W, H, &dims(Some(200), Some(200)), false),
            Some((200, 200))
        );
    }

    #[test]
    fn exact_dimensions_upscale_clamped() {
        // Both dimensions grow -> no-op without --allow-upscale.
        assert_eq!(
            compute_target(W, H, &dims(Some(2000), Some(900)), false),
            None
        );
        assert_eq!(
            compute_target(W, H, &dims(Some(2000), Some(900)), true),
            Some((2000, 900))
        );
    }

    #[test]
    fn exact_dimensions_mixed_clamp() {
        // Width shrinks, height would grow -> height clamped to original.
        assert_eq!(
            compute_target(W, H, &dims(Some(500), Some(900)), false),
            Some((500, 500))
        );
    }

    #[test]
    fn width_only_keeps_aspect() {
        assert_eq!(
            compute_target(W, H, &dims(Some(500), None), false),
            Some((500, 250))
        );
    }

    #[test]
    fn height_only_keeps_aspect() {
        assert_eq!(
            compute_target(W, H, &dims(None, Some(250)), false),
            Some((500, 250))
        );
    }

    #[test]
    fn width_only_upscale_is_noop_unless_allowed() {
        assert_eq!(compute_target(W, H, &dims(Some(2000), None), false), None);
        assert_eq!(
            compute_target(W, H, &dims(Some(2000), None), true),
            Some((2000, 1000))
        );
    }

    #[test]
    fn same_size_is_noop() {
        assert_eq!(
            compute_target(W, H, &dims(Some(1000), Some(500)), false),
            None
        );
        assert_eq!(compute_target(W, H, &dims(Some(1000), None), false), None);
        assert_eq!(
            compute_target(W, H, &ResizeSpec::Percent(100.0), false),
            None
        );
    }

    #[test]
    fn percent_scales() {
        assert_eq!(
            compute_target(W, H, &ResizeSpec::Percent(50.0), false),
            Some((500, 250))
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::Percent(10.0), false),
            Some((100, 50))
        );
    }

    #[test]
    fn percent_upscale() {
        assert_eq!(
            compute_target(W, H, &ResizeSpec::Percent(150.0), false),
            None
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::Percent(150.0), true),
            Some((1500, 750))
        );
    }

    #[test]
    fn percent_rounding_never_zero() {
        assert_eq!(
            compute_target(3, 3, &ResizeSpec::Percent(1.0), false),
            Some((1, 1))
        );
    }

    #[test]
    fn percent_rounding_half_up() {
        // 101 * 0.5 = 50.5 -> 51, 77 * 0.5 = 38.5 -> 39
        assert_eq!(
            compute_target(101, 77, &ResizeSpec::Percent(50.0), false),
            Some((51, 39))
        );
    }

    #[test]
    fn max_width() {
        assert_eq!(
            compute_target(W, H, &ResizeSpec::MaxWidth(400), false),
            Some((400, 200))
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::MaxWidth(1200), false),
            None
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::MaxWidth(1000), false),
            None
        );
    }

    #[test]
    fn max_height() {
        assert_eq!(
            compute_target(W, H, &ResizeSpec::MaxHeight(250), false),
            Some((500, 250))
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::MaxHeight(600), false),
            None
        );
    }

    #[test]
    fn longest_edge() {
        assert_eq!(
            compute_target(W, H, &ResizeSpec::LongestEdge(500), false),
            Some((500, 250))
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::LongestEdge(1500), false),
            None
        );
        // Portrait orientation: longest edge is the height.
        assert_eq!(
            compute_target(500, 1000, &ResizeSpec::LongestEdge(500), false),
            Some((250, 500))
        );
    }

    #[test]
    fn shortest_edge() {
        assert_eq!(
            compute_target(W, H, &ResizeSpec::ShortestEdge(250), false),
            Some((500, 250))
        );
        // Shortest edge already equals target.
        assert_eq!(
            compute_target(W, H, &ResizeSpec::ShortestEdge(500), false),
            None
        );
        // Would upscale -> no-op unless allowed.
        assert_eq!(
            compute_target(W, H, &ResizeSpec::ShortestEdge(800), false),
            None
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::ShortestEdge(800), true),
            Some((1600, 800))
        );
    }

    #[test]
    fn max_megapixels() {
        // 1000x500 = 0.5 MP; cap at 0.25 MP -> scale by sqrt(0.5) ~ 0.7071.
        assert_eq!(
            compute_target(W, H, &ResizeSpec::MaxMegapixels(0.25), false),
            Some((707, 354))
        );
        assert_eq!(
            compute_target(W, H, &ResizeSpec::MaxMegapixels(1.0), false),
            None
        );
    }

    #[test]
    fn zero_original_is_noop() {
        assert_eq!(
            compute_target(0, 0, &ResizeSpec::Percent(50.0), false),
            None
        );
    }
}
