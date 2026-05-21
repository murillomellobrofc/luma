# Luma — Volume & Brightness OSD — Specification

Component: H45.osd
Status: spec, ready for implementation

---

## 1. Purpose

The OSD (On-Screen Display) is the small indicator that appears when
the user presses a volume or brightness key on the keyboard. It shows
the current level and disappears on its own after a short time.

Today the KDE OSD shows through (a leftover from the host system).
Luma needs its own OSD with the Golden Hour identity.

Volume and brightness use the SAME component. Only the icon and the
data source change. They are built together.

---

## 2. Visual layout

A dark rounded pill, 260 x 64 px, horizontally centered, placed just
above the taskbar.

```
        +----------------------------+
        |  [icon]  =========----  68% |
        +----------------------------+
        ========== taskbar ===========
```

Three parts inside the pill:

- Icon, left side. Volume waves or a sun, depending on the mode.
- Track, center. A thin bar; the filled portion shows the level.
- Percentage, right side. The numeric level.

---

## 3. States

### Normal
Track filled in amber proportional to the level. Percentage in honey.
The volume icon shows 1, 2 or 3 waves depending on how high the level
is.

### Muted
Track empty. Icon turns coral with an X mark. The label reads "Muted"
instead of a percentage.

### Above 100 percent
Only relevant for volume. The track fill and the percentage both turn
coral, signaling that the level is above the safe range.

### Icon by level (volume)
- 0 percent or muted: muted icon
- low: speaker with 1 wave
- mid: speaker with 2 waves
- high: speaker with 3 waves

### Brightness
Same pill, same track, same percentage. The icon is a sun. No
above-100 state for brightness.

---

## 4. Behavior

- Appears when a volume or brightness key is pressed.
- Stays visible for about 1.5 seconds after the last keypress.
- Each new keypress resets the timer. The OSD updates its value in
  place; it does not flicker or re-spawn.
- Dismisses with a smooth fade-out, never a hard cut.
- Always on top of other windows.
- Never steals keyboard focus.
- Never blocks mouse clicks on whatever is underneath it.

---

## 5. Theme tokens

All values live in `~/.config/luma/theme.toml`. No hardcoded values
in the Rust code.

```toml
[osd]                       # shared by volume and brightness
width          = 260
height         = 64
bg             = "#1F1611"
corner_radius  = 16
margin_bottom  = 90         # gap from the bottom edge, above taskbar
shadow_blur    = 16
shadow_opacity = 0.45
timeout_ms     = 1500       # visible time after the last keypress
fade_ms        = 200        # fade-out duration

[osd.track]
bg             = "#3D2520"  # empty track
fill           = "#E89B3D"  # filled portion, amber
fill_over      = "#D97757"  # filled portion when volume > 100%, coral
height         = 6

[osd.text]
color          = "#F5D5A8"  # percentage, normal
color_over     = "#D97757"  # percentage when > 100%
color_muted    = "#D97757"  # the "Muted" label
size           = 14
```

---

## 6. Keys to capture

- XF86AudioRaiseVolume — volume up
- XF86AudioLowerVolume — volume down
- XF86AudioMute — toggle mute
- XF86MonBrightnessUp — brightness up
- XF86MonBrightnessDown — brightness down

---

## 7. Data sources

### Volume
Reuse the read/set logic already built for the volume applet (H44).
The OSD only needs the current level and the muted flag.

### Brightness
Read and write the backlight through sysfs:
- current: `/sys/class/backlight/<device>/brightness`
- maximum: `/sys/class/backlight/<device>/max_brightness`
The percentage is current divided by maximum.

---

## 8. Build plan

Three phases. Atomic commits, each compiles and runs.

Phase 1 — Investigation. Map how keys are captured today, how the H44
volume logic can be reused, how to read the backlight, and how to draw
a non-focus overlay. Report before coding.

Phase 2 — Implementation:
- Block A: the OSD component (surface, content, timer, fade-out)
- Block B: volume OSD (key capture, icon by level, muted and over-100
  states)
- Block C: brightness OSD (key capture, backlight read/write, sun icon)
- Block D: polish and validation

Phase 3 — Validation against the checklist.

---

## 9. Done criteria

- Volume key shows the OSD centered above the taskbar
- Brightness key shows the same OSD with a sun icon
- Auto-dismisses after about 1.5 seconds
- Pressing again resets the timer and updates in place
- Smooth fade-out
- Volume icon changes with the level
- Muted and above-100 states render correctly
- Does not steal focus, does not block clicks underneath
- The KDE OSD no longer shows on top
- `cargo build` passes
