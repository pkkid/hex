//! Labs › Annotate on GNOME. A Wayland compositor without layer-shell
//! (GNOME) has no surface for an overlay that takes clicks, so Annotate's
//! recording capsule (with its pen) and the ink overlay can't open there, and
//! a capture records with nothing to draw on. With this explicit opt-in, Rune
//! opens them on XWayland as override-redirect windows that take clicks only
//! in their input shape and never keyboard focus, and only while this user's
//! XWayland already runs: Hex never starts it. The rules are the meeting
//! capsules' (`linux::meeting_capsules`), under their own switch.

/// Whether Annotate's capsule and ink may float on XWayland, read as each
/// shows. With the lab off it looks for nothing.
pub(crate) fn on_xwayland() -> bool {
    super::meeting_capsules::floats_on_xwayland(
        crate::labs::gnome_annotate(),
        super::meeting_capsules::without_layer_shell,
        crate::linux::clipboard::xwayland_running,
    )
}
