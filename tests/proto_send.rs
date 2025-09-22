#![cfg(feature = "meshtastic-proto")]

// Only import device when serial feature is disabled so we actually use it in the test below.
#[cfg(all(feature = "meshtastic-proto", not(feature = "serial")))]
use meshbbs::meshtastic::MeshtasticDevice;

// Helper: construct a device without serial (if serial feature off test still compiles but skipped)
// Only build this test when meshtastic-proto is on and serial is off so we can construct a mock device.
#[cfg(all(feature = "meshtastic-proto", not(feature = "serial")))]
#[tokio::test]
async fn build_text_packet_encodes() {
    let mut dev = MeshtasticDevice::new("/dev/null", 115200).await.expect("create device");
    dev.send_text_packet(Some(0x12345678), 0, "TEST").expect("send packet");
}

// When serial feature is enabled we provide a no-op test to keep test count stable without warning noise.
#[cfg(all(feature = "meshtastic-proto", feature = "serial"))]
#[tokio::test]
async fn build_text_packet_encodes() { /* skipped: requires real hardware */ }
