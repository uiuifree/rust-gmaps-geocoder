//! Minimal forward + reverse geocoding against the real API.
//!
//! Run it:
//! ```sh
//! GOOGLE_MAPS_API_KEY=xxxx cargo run --example geocode -- "1600 Amphitheatre Parkway, Mountain View, CA"
//! ```
//! This is the only place in the repository that calls Google; the test suite runs
//! entirely against a mock server.
use uiuifree_gmaps_geocoder::GeocodingClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key =
        std::env::var("GOOGLE_MAPS_API_KEY").expect("set the GOOGLE_MAPS_API_KEY env var");
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "東京都渋谷区道玄坂1-1-1".to_string());

    let client = GeocodingClient::new(api_key);

    println!("== forward: {address} ==");
    let results = client.geocode(&address).await?;
    if results.is_empty() {
        println!("no match (ZERO_RESULTS)");
        return Ok(());
    }
    for r in &results {
        let loc = r.location();
        println!(
            "{}  ({}, {})  [{}]",
            r.formatted_address, loc.lat, loc.lng, r.geometry.location_type
        );
        println!(
            "    postal={:?}  area={:?} city={:?}",
            r.postal_code(),
            r.prefecture(),
            r.locality(),
        );
    }

    // Feed the first coordinate straight back in, to show the reverse direction.
    let loc = results[0].location();
    println!("\n== reverse: {}, {} ==", loc.lat, loc.lng);
    for r in client
        .reverse_geocode(loc.lat, loc.lng)
        .await?
        .iter()
        .take(3)
    {
        println!("{}", r.formatted_address);
    }

    Ok(())
}
