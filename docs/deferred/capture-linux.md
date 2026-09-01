# Spec — Route B capture: Linux

Satisfies R3, R4. **Sketch only** — lowest priority, Phase 6.

## Mechanism

PipeWire. Discord appears as a node with output ports; capture its monitor
rather than the whole sink, which keeps R4 intact.

```
find node where application.name / application.process.binary ~ Discord
        ▼
create a capture stream linked to that node's monitor ports
        ▼
on_process callback ──▶ Frame::Pcm
```

`libpipewire` directly, or `pipewire-rs` from Rust.

PulseAudio (`module-loopback` against a sink monitor) is a fallback for older
systems and captures the whole sink rather than one application — meaning R4
cannot hold there. Record which path was used in the manifest.

## Notes

- Sample rate is negotiated; request 48 kHz to match Discord and avoid a
  resample.
- Discord may run as Flatpak or Snap, which changes how the node is identified
  and may sandbox access. Test all three packagings.
- Device changes arrive as graph events; handle them the same way as Windows
  (rebuild the stream, keep the session).

## Open questions

- Reliable identification of Discord's node across native, Flatpak and Snap.
- Whether Wayland session restrictions affect PipeWire audio capture, or only
  screen capture.
