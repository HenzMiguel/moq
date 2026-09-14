// cargo run --example asymmetric
//
// Demonstrates asymmetric key usage where the private key signs tokens
// and the public key verifies them. This is the recommended approach for
// production: the relay only needs the public key.

use std::time::{Duration, SystemTime};

fn main() -> anyhow::Result<()> {
	// Generate an ECDSA P-256 key pair.
	let private_key = moq_token::Key::generate(moq_token::Algorithm::ES256, Some(moq_token::KeyId::random()))?;
	println!("Private key:\n{}\n", private_key.to_str()?);

	// Extract the public key for the relay.
	let public_key = private_key.to_public()?;
	println!("Public key (give this to the relay):\n{}\n", public_key.to_str()?);

	// Sign a token with the private key.
	let claims = moq_token::Claims::V0(
		moq_token::ClaimsV0::default()
			.with_root("rooms/meeting-123")
			.with_publish(["alice"])
			.with_subscribe([""])
			.with_expires(SystemTime::now() + Duration::from_secs(3600))
			.with_issued(SystemTime::now()),
	);

	let token = private_key.sign(&claims)?;
	println!("Signed token:\n{token}\n");

	// Verify with the public key (this is what the relay does).
	let verified = public_key.verify(&token)?;
	let verified_v0 = verified.as_v0().expect("v0 token");
	println!("Verified with public key:");
	println!("  root: {}", verified.root());
	println!("  publish: {:?}", verified_v0.publish);
	println!("  subscribe: {:?}", verified_v0.subscribe);

	Ok(())
}
