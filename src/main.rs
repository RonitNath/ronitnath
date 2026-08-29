use askama::Template;
use std::{fs, path::Path};

#[derive(Template)]
#[template(path = "isoastra.html")]
struct Isoastra<'a> {
    title: &'a str,
    description: &'a str,
    page: &'a str,
    body: &'a str,
}

#[derive(Template)]
#[template(path = "rinity.html")]
struct Rinity;

#[derive(Template)]
#[template(path = "ronitnath.html")]
struct RonitNath;

fn write(path: impl AsRef<Path>, content: impl AsRef<[u8]>) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn main() {
    let iso_pages = [
        ("index.html", "Isoastra — Applied intelligence", "Distributed AI systems for knowledge work and robotics.", "home", ""),
        ("who-we-are/index.html", "Who We Are — Isoastra", "An independent technology company building applied AI systems and focused software products.", "who", ""),
        ("portfolio/index.html", "Portfolio — Isoastra", "Companies and systems built for consequential work.", "portfolio", ""),
        ("careers/index.html", "Careers — Isoastra", "Build difficult things with a small team.", "careers", ""),
        ("privacy/index.html", "Privacy Policy — Isoastra", "How Isoastra handles personal information.", "document", r#"
          <h1>Privacy Policy</h1><p class="eyebrow">Last updated August 15, 2026</p>
          <p>This policy explains how Isoastra handles personal information through isoastra.com and in our general business operations.</p>
          <h2>Information we collect</h2><p>We may receive information you provide directly, ordinary website request data, and business correspondence. Product-specific services may publish supplemental notices.</p>
          <h2>How we use information</h2><p>We use information to provide and secure our services, communicate, operate the company, comply with law, and improve reliability.</p>
          <h2>Sharing and retention</h2><p>We disclose information to service providers and professional advisers only as needed, or when required by law. We retain it only as long as necessary for its purpose and obligations.</p>
          <h2>Contact</h2><p>Questions may be sent to <a href="mailto:legal@isoastra.com">legal@isoastra.com</a>.</p>"#),
        ("terms/index.html", "Terms of Service — Isoastra", "Terms for Isoastra websites and services.", "document", r#"
          <h1>Terms of Service</h1><p class="eyebrow">Last updated August 15, 2026</p>
          <p>These Terms are an agreement between you and Isoastra. By accessing isoastra.com or a service incorporating these Terms, you agree to them.</p>
          <h2>Services and acceptable use</h2><p>Isoastra operates websites, software, and product brands. You may not misuse a service, interfere with its operation, or violate applicable law.</p>
          <h2>Intellectual property</h2><p>Isoastra and its licensors retain all rights in our websites, services, software, designs, and content.</p>
          <h2>Disclaimers and liability</h2><p>Services are provided as available. To the fullest extent permitted by law, Isoastra is not liable for indirect, incidental, special, consequential, exemplary, or punitive damages.</p>
          <h2>Contact</h2><p>Questions may be sent to <a href="mailto:legal@isoastra.com">legal@isoastra.com</a>.</p>"#),
    ];
    for (path, title, description, page, body) in iso_pages {
        write(
            Path::new("dist/isoastra").join(path),
            Isoastra { title, description, page, body }.render().unwrap(),
        );
    }
    write("dist/rinity/index.html", Rinity.render().unwrap());
    write("dist/ronitnath/index.html", RonitNath.render().unwrap());
}

