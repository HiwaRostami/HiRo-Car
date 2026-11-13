# HiRo Car ELM327 adapter intelligence audit

## Current architecture

- The working tree is `D:\obdium-main`. It has no Git metadata, so branch, commit, and working-tree cleanliness cannot be verified.
- `backend/Cargo.toml` defines a Rust 2021 application with Tauri 2 and SQLite. `frontend/index.html`, `frontend/scripts`, and `frontend/locales` supply the HTML, CSS, and JavaScript interface in Persian and English. `backend/gen/android` holds the Android project. `backend/data` holds replay data and lookup databases. `examples` and `docs` hold screenshots. The existing publishing workflow targets desktop.
- Android uses Bluetooth Classic RFCOMM in `ElmBluetooth.kt`, through JNI in `backend/src/bluetooth.rs`. The Android socket tries standard SPP, a compatibility socket, and an existing direct-channel fallback. `AdapterPort` in `backend/src/port.rs` already abstracts Android Bluetooth, desktop serial, and demo transport. Reuse this boundary for mock and future transports.
- `backend/src/obd.rs` sends commands defined by `cmd.rs`, initializes the adapter, selects a protocol, and probes `0100` on Android. Responses are read across Bluetooth reads until the ELM prompt. The current reader uses deadlines but returns errors without a structured trace of partial data.
- `backend/src/stats.rs` and `backend/src/bridge/events.rs` share the session through a mutex and pause counter. This serializes access but does not provide a bounded request queue, cancellation, or a single recovery policy. Bluetooth operations and some polling run synchronously and can delay other requests.
- `backend/src/frames.rs` assembles framed responses by ECU. `response.rs`, `scalar.rs`, and `pid/` implement payload decoding and units. Preserve these components and use fixtures to clarify framing coverage and response validation.
- Existing diagnostics include Mode 01 live values and PID discovery, partial Mode 02 support, stored/pending/permanent DTCs, clearing, VIN and a partial monitor-data path. Mode 05/06 coverage is incomplete, and there is no validated Mode 08 implementation. Freeze-frame responses need explicit parsing and tests. Scope each PID bitmap to its responding ECU and validate reply service and PID against the request.
- The connection screen, live dashboard, terminal and ECU JSON export are useful integration points. `replay.rs` stores request/response fixtures but currently rewrites a JSON array and lacks timing, classification and bounded retention. Keep fixture replay separate from a user-enabled diagnostic trace.

## Risks and implementation order

1. Introduce a bounded per-session command executor over `AdapterPort`. Keep one request in flight. Test fragmented and multiline responses, exact echo removal, prompt detection, timeouts and disconnects with a scripted transport. Cancel queued work on disconnect and drain or reset a desynchronized session before accepting the next request. Restrict retry to appropriate read operations.
2. Capture the existing initialization result and probe adapter identity via documented read commands. Keep claimed firmware, command acceptance and independently observed communication as distinct evidence. Include duration, raw response, parsed result and status for each probe. Avoid resetting an active session merely to display information.
3. Report capabilities conservatively. An adapter accepting a configuration command only demonstrates command acceptance. Mark vehicle protocol verification after a valid ECU response and show the reported active protocol separately. Represent unavailable, untested, partial and hardware-dependent results explicitly. Keep protocol-changing probes outside routine discovery.
4. Validate existing standard modes against fixtures before adding coverage. Separate generic framing from vehicle definitions. Restrict active controls to explicitly authorized, validated workflows. Keep arbitrary requests, security access, ECU coding, flashing and actuator operations outside automatic discovery.
5. Start the Samand EF7 profile with standard read-only PIDs gated by supported bitmaps. Treat the supplied Bosch ME7.4.9 and dual-fuel configuration as test targets. Add CNG fields only when a verified definition and compatible transport are available. Keep missing CNG definitions and unknown availability visible.
6. Integrate adapter identity and capability views with the existing bilingual UI. Provide opt-in, size-bounded JSON/CSV trace export with sensitive fields such as VIN and adapter identifiers omitted or redacted by default. Verify offline with fixtures before validating on the actual Samand.

## Hardware compatibility boundaries

An ATI string is a firmware claim, including when it says ELM327 V2.1. It cannot establish authenticity, PIC lineage, electrical support or the complete command set. A clone may answer basic queries and omit advanced commands. STN implementations and USB interfaces need their own transport and command evidence. A native CAN interface can use a different transport entirely. Software cannot supply a missing transceiver or wire, make a vehicle expose a PID, or grant ECU permissions. The Android RFCOMM path currently does not establish BLE or USB support.

## Baseline validation and build

- The repository provides two frame tests and recorded demo data. A physical adapter or vehicle is not required for the frame tests.
- From `backend`, the existing Android build entry point is `cargo tauri android build --apk --target aarch64`. Desktop build and dev commands are in `README.md`. Android builds additionally depend on the local SDK, NDK, and Gradle.
- No hardware capabilities are established by this audit. The user's prior successful connection is evidence for that particular connection only. Advanced probing, protocol coverage and CNG communication remain to be validated.
