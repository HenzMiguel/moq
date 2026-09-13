//! An application embeds the relay: custom HTTP routes plus a worker that
//! publishes into [`Cluster::origin`].
//!
//! Smoke cannot host this. It installs published clients, not the relay crate.
//! The owning runner that keeps `workers`/`uring` from being dropped is
//! [quest/m1/api-relay-embedding.md](../../../quest/m1/api-relay-embedding.md);
//! this test uses the current `Relay::load` pieces with `runtime.workers` unset,
//! so the `..` remainder does not hold bound QUIC sockets.

use std::{net::TcpListener, time::Duration};

use axum::routing::get;
use moq_relay::{Config, PublicConfig, Relay};
use moq_tokio::moq_net::{self, Hop, Timestamp};

const TIMEOUT: Duration = Duration::from_secs(10);

fn free_tcp_port() -> u16 {
	let probe = TcpListener::bind("127.0.0.1:0").expect("bind probe");
	let port = probe.local_addr().expect("local addr").port();
	drop(probe);
	port
}

fn client() -> moq_tokio::Client {
	let mut config = moq_tokio::connect::Config::default();
	config.tls.insecure = Some(true);
	config.once = Some(true);
	config.websocket.delay = Duration::ZERO.into();
	config.bind = Some("127.0.0.1:0".parse().expect("parse bind"));
	config.init(Default::default()).expect("client init")
}

#[tokio::test]
async fn embedder_custom_route_and_origin_worker() {
	let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

	let port = free_tcp_port();
	let mut config = Config::default();
	config.listen.bind = Some("127.0.0.1:0".to_string());
	config.listen.tls.generate = vec!["localhost".into()];
	config.web.ws = true;
	config.web.http.listen = Some(format!("127.0.0.1:{port}").parse().expect("parse listen"));
	#[allow(deprecated)]
	{
		config.auth.public = Some(PublicConfig::Simple(vec![String::new()]));
	}

	let Relay {
		web,
		cluster,
		shutdown: _,
		shutdown_trigger,
		..
	} = Relay::load(config).await.expect("load relay");

	let app = web.routes().route("/app", get(|| async { "app\n" }));
	let (server_result_tx, mut server_result_rx) = tokio::sync::oneshot::channel();
	let web_handle = tokio::spawn(async move {
		let _ = server_result_tx.send(web.serve(app).await);
	});

	let deadline = std::time::Instant::now() + Duration::from_secs(5);
	loop {
		if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
			break;
		}
		match server_result_rx.try_recv() {
			Ok(Ok(())) => panic!("web server exited before listening"),
			Ok(Err(err)) => panic!("web server failed before listening: {err:#}"),
			Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
			Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
				panic!("web server task ended before listening")
			}
		}
		if std::time::Instant::now() >= deadline {
			panic!("http listener never became ready on port {port}");
		}
		tokio::time::sleep(Duration::from_millis(25)).await;
	}

	let body = reqwest::get(format!("http://127.0.0.1:{port}/app"))
		.await
		.expect("fetch /app")
		.text()
		.await
		.expect("read /app");
	assert_eq!(body, "app\n");

	let mut broadcast = cluster.origin.create_broadcast("app.worker").expect("create broadcast");
	broadcast.announce(Default::default()).expect("announce");
	let mut track = broadcast.create_track("ping", None).expect("create track");
	let mut group = track.append_group().expect("append group");
	group
		.write_frame(Timestamp::ZERO, b"pong".as_ref())
		.expect("write frame");
	group.finish().expect("finish group");

	let url: url::Url = format!("ws://127.0.0.1:{port}/").parse().expect("parse url");
	let sub_origin = moq_tokio::origin::spawn(Hop::random());
	let sub_consumer = sub_origin.consume();
	let mut announcements = sub_consumer.announced();
	let session = tokio::time::timeout(TIMEOUT, client().with_subscriber(sub_origin).connect(url).established())
		.await
		.expect("subscriber connect timeout")
		.expect("subscriber connect failed");

	let update = tokio::time::timeout(TIMEOUT, announcements.next())
		.await
		.expect("announcement timeout")
		.expect("origin closed");
	assert!(update.active, "expected announce, got retraction");
	let path = moq_net::Path::new(update.pattern.as_prefix().expect("prefix announcement")).to_owned();
	assert_eq!(path.as_str(), "app.worker");
	let bc = sub_consumer
		.request_broadcast(&path)
		.await
		.expect("announced broadcast resolves");
	let mut track_sub = bc.track("ping").unwrap().subscribe(None).await.expect("subscribe");
	let mut group_sub = tokio::time::timeout(TIMEOUT, track_sub.recv_group())
		.await
		.expect("recv_group timeout")
		.expect("recv_group failed")
		.expect("track closed prematurely");
	let frame = tokio::time::timeout(TIMEOUT, group_sub.read_frame())
		.await
		.expect("read_frame timeout")
		.expect("read_frame failed")
		.expect("group closed prematurely");
	assert_eq!(&frame.payload[..], b"pong");

	drop(session);
	drop(track);
	drop(broadcast);
	shutdown_trigger.start();
	web_handle.abort();
	let _ = web_handle.await;
	drop(cluster);

	let deadline = std::time::Instant::now() + Duration::from_secs(2);
	loop {
		if TcpListener::bind(("127.0.0.1", port)).is_ok() {
			break;
		}
		if std::time::Instant::now() >= deadline {
			panic!("stopping the embedder should release the HTTP listener");
		}
		tokio::time::sleep(Duration::from_millis(25)).await;
	}
}
