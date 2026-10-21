# LiveKit WebRTC binary notices

`FloeNative` pins LiveKit's `webrtc-xcframework` Swift package to release
`150.7871.03` (resolved source revision `a41df9e9bcc7ab1d62131fda75c123ea391e5188`).
The package's release `LICENSE` is MIT; a copy is retained in the bundled
framework's `ThirdPartyLicenses` directory.

The release README identifies `webrtc-sdk/webrtc` and `webrtc-sdk/webrtc-build`
as build sources. The WebRTC SDK source `m150_release` supplies a BSD-3-Clause
license, reproduced here for binary distributions. The upstream build and
release manifests are authoritative for any additional bundled components and
notices; re-audit them when changing the pinned release.

- [Pinned package manifest](https://github.com/livekit/webrtc-xcframework/blob/150.7871.03/Package.swift)
- [Pinned package license](https://github.com/livekit/webrtc-xcframework/blob/150.7871.03/LICENSE)
- [Pinned package README](https://github.com/livekit/webrtc-xcframework/blob/150.7871.03/README.md)
- [WebRTC SDK BSD-3-Clause license](https://github.com/webrtc-sdk/webrtc/blob/m150_release/LICENSE)

The current muted lease creates an offer-capable audio transceiver without an
audio source or track. It does not request microphone authorization or capture
audio. A future capture implementation must add its actual permission-purpose
string and user-facing authorization path before enabling capture.
