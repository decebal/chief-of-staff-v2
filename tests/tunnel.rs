use cos::tunnel::find_url;

#[test]
fn finds_quick_tunnel_url_in_cloudflared_banner() {
    let line = "2026-10-16T11:30:01Z INF |  https://brave-otter-lake-sun.trycloudflare.com                    |";
    assert_eq!(
        find_url(line).as_deref(),
        Some("https://brave-otter-lake-sun.trycloudflare.com")
    );
}

#[test]
fn ignores_other_https_urls() {
    let line = "2026-10-16T11:30:00Z INF Thank you for trying Cloudflare Tunnel. See https://developers.cloudflare.com/cloudflare-one/";
    assert_eq!(find_url(line), None);
}
