//! Lightweight built-in dashboard for rszero services.
//!
//! Provides a real-time monitoring UI using HTMX + Alpine.js + Tailwind CSS
//! with zero build step — everything is served via CDN.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use rszero::rest::server::RszeroServer;
//!
//! let server = RszeroServer::new("0.0.0.0", 8080)
//!     .route("/hello", axum::routing::get(hello))
//!     .with_dashboard(); // mounts /dashboard
//! ```
//!
//! # Endpoints
//!
//! | Path | Description |
//! |------|-------------|
//! | `GET /dashboard` | Main HTML page |
//! | `GET /dashboard/api/metrics` | Metrics cards (HTML fragment) |
//! | `GET /dashboard/api/health` | Health status table (HTML fragment) |
//! | `GET /dashboard/api/system` | System info (HTML fragment) |
//! | `GET /dashboard/api/app` | Application info (HTML fragment) |
//! | `GET /dashboard/api/routes` | Registered routes (HTML fragment) |

use axum::response::Html;
use axum::{routing::get, Router};

mod data;

pub use data::{record_error, record_request, RouteRegistry};

/// Mount dashboard routes onto the given router.
pub fn mount(router: Router) -> Router {
    router
        .route("/dashboard", get(dashboard_page_handler))
        .route("/dashboard/api/metrics", get(metrics_fragment_handler))
        .route("/dashboard/api/health", get(health_fragment_handler))
        .route("/dashboard/api/system", get(system_fragment_handler))
        .route("/dashboard/api/app", get(app_fragment_handler))
        .route("/dashboard/api/routes", get(routes_fragment_handler))
}

/// Serve the main dashboard HTML page.
async fn dashboard_page_handler() -> Html<&'static str> {
    Html(include_str!("page.html"))
}

/// Metrics cards fragment (HTMX target).
async fn metrics_fragment_handler() -> Html<String> {
    let m = data::MetricsSnapshot::collect();
    let html = format!(
        r##"
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 card-hover transition-all duration-200">
          <div class="flex items-center justify-between mb-2">
            <span class="text-slate-400 text-sm">Uptime</span>
            <i class="fas fa-clock text-blue-400"></i>
          </div>
          <div class="text-2xl font-bold text-white">{}</div>
        </div>
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 card-hover transition-all duration-200">
          <div class="flex items-center justify-between mb-2">
            <span class="text-slate-400 text-sm">Total Requests</span>
            <i class="fas fa-arrow-up text-emerald-400"></i>
          </div>
          <div class="text-2xl font-bold text-white">{}</div>
        </div>
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 card-hover transition-all duration-200">
          <div class="flex items-center justify-between mb-2">
            <span class="text-slate-400 text-sm">Errors</span>
            <i class="fas fa-exclamation-triangle text-rose-400"></i>
          </div>
          <div class="text-2xl font-bold text-white">{}</div>
        </div>
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 card-hover transition-all duration-200">
          <div class="flex items-center justify-between mb-2">
            <span class="text-slate-400 text-sm">Error Rate</span>
            <i class="fas fa-percentage text-amber-400"></i>
          </div>
          <div class="text-2xl font-bold {}">{:.2}%</div>
        </div>
        "##,
        m.uptime_formatted,
        m.total_requests,
        m.error_requests,
        if m.error_rate > 5.0 { "text-rose-400" } else { "text-emerald-400" },
        m.error_rate
    );
    Html(html)
}

/// Health status table fragment.
async fn health_fragment_handler() -> Html<String> {
    let health = crate::health::Health::new();
    let status = data::HealthStatus::from_health(&health);

    let rows: String = status
        .checks
        .iter()
        .map(|c| {
            let (icon, color) = if c.status == "healthy" {
                ("fa-check-circle", "text-emerald-400")
            } else {
                ("fa-times-circle", "text-rose-400")
            };
            format!(
                r#"<tr class="border-b border-slate-800">
                    <td class="py-3 px-5 text-slate-300">{}</td>
                    <td class="py-3 px-5">
                      <span class="flex items-center gap-2 {}">
                        <i class="fas {}"></i> {}
                      </span>
                    </td>
                    <td class="py-3 px-5 text-slate-500 text-sm">{}</td>
                  </tr>"#,
                c.name,
                color,
                icon,
                c.status,
                c.message.as_deref().unwrap_or("—")
            )
        })
        .collect();

    let overall_icon = if status.overall {
        ("fa-check-circle", "text-emerald-400", "All systems operational")
    } else {
        ("fa-exclamation-circle", "text-rose-400", "Some systems unhealthy")
    };

    let html = format!(
        r##"
        <div class="px-5 py-3 bg-slate-900/50 border-b border-slate-800 flex items-center gap-3">
          <i class="fas {}"></i>
          <span class="font-medium text-white">{}</span>
        </div>
        <table class="w-full text-sm text-left">
          <thead class="text-xs text-slate-500 uppercase bg-slate-900/50">
            <tr>
              <th class="py-2 px-5">Component</th>
              <th class="py-2 px-5">Status</th>
              <th class="py-2 px-5">Message</th>
            </tr>
          </thead>
          <tbody>{}</tbody>
        </table>
        "##,
        overall_icon.0, overall_icon.2, rows
    );
    Html(html)
}

/// System info fragment.
async fn system_fragment_handler() -> Html<String> {
    let sys = data::SystemInfo::collect();
    let html = format!(
        r##"
        <div class="space-y-3 text-sm">
          <div class="flex justify-between border-b border-slate-800 pb-2">
            <span class="text-slate-500">Operating System</span>
            <span class="text-slate-200 font-mono">{}</span>
          </div>
          <div class="flex justify-between border-b border-slate-800 pb-2">
            <span class="text-slate-500">Architecture</span>
            <span class="text-slate-200 font-mono">{}</span>
          </div>
          <div class="flex justify-between border-b border-slate-800 pb-2">
            <span class="text-slate-500">Rust Version</span>
            <span class="text-slate-200 font-mono">{}</span>
          </div>
          <div class="flex justify-between border-b border-slate-800 pb-2">
            <span class="text-slate-500">Process ID</span>
            <span class="text-slate-200 font-mono">{}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-slate-500">Uptime</span>
            <span class="text-slate-200 font-mono">{}s</span>
          </div>
        </div>
        "##,
        sys.os, sys.arch, sys.rust_version, sys.pid, sys.uptime_seconds
    );
    Html(html)
}

/// Application info fragment.
async fn app_fragment_handler() -> Html<String> {
    let app = data::AppInfo::collect();
    let html = format!(
        r##"
        <div class="grid grid-cols-1 sm:grid-cols-3 gap-4 text-sm">
          <div class="bg-slate-950 rounded-lg p-4 border border-slate-800">
            <div class="text-slate-500 mb-1">Application</div>
            <div class="text-white font-semibold">{} <span class="text-slate-500 font-normal">v{}</span></div>
          </div>
          <div class="bg-slate-950 rounded-lg p-4 border border-slate-800">
            <div class="text-slate-500 mb-1">Uptime</div>
            <div class="text-white font-semibold font-mono">{}s</div>
          </div>
          <div class="bg-slate-950 rounded-lg p-4 border border-slate-800">
            <div class="text-slate-500 mb-1">Request Stats</div>
            <div class="text-white font-semibold">
              <span class="text-emerald-400">{}</span> ok
              <span class="text-slate-600 mx-1">/</span>
              <span class="text-rose-400">{}</span> err
            </div>
          </div>
        </div>
        "##,
        app.name, app.version, app.uptime_seconds,
        app.total_requests - app.error_requests,
        app.error_requests
    );
    Html(html)
}

/// Registered routes fragment.
async fn routes_fragment_handler() -> Html<String> {
    // For now, show a placeholder. In a future iteration we can integrate
    // with axum's route introspection.
    let html = r##"
    <table class="w-full text-sm text-left">
      <thead class="text-xs text-slate-500 uppercase bg-slate-900/50">
        <tr>
          <th class="py-2 px-5">Method</th>
          <th class="py-2 px-5">Path</th>
        </tr>
      </thead>
      <tbody>
        <tr class="border-b border-slate-800">
          <td class="py-3 px-5"><span class="px-2 py-0.5 rounded text-xs bg-blue-500/10 text-blue-400 font-mono">GET</span></td>
          <td class="py-3 px-5 text-slate-300 font-mono">/dashboard</td>
        </tr>
        <tr class="border-b border-slate-800">
          <td class="py-3 px-5"><span class="px-2 py-0.5 rounded text-xs bg-emerald-500/10 text-emerald-400 font-mono">GET</span></td>
          <td class="py-3 px-5 text-slate-300 font-mono">/health</td>
        </tr>
        <tr class="border-b border-slate-800">
          <td class="py-3 px-5"><span class="px-2 py-0.5 rounded text-xs bg-emerald-500/10 text-emerald-400 font-mono">GET</span></td>
          <td class="py-3 px-5 text-slate-300 font-mono">/ready</td>
        </tr>
        <tr>
          <td class="py-3 px-5"><span class="px-2 py-0.5 rounded text-xs bg-blue-500/10 text-blue-400 font-mono">GET</span></td>
          <td class="py-3 px-5 text-slate-300 font-mono">/metrics</td>
        </tr>
      </tbody>
    </table>
    <div class="px-5 py-3 text-xs text-slate-600 border-t border-slate-800">
      Register custom routes via <code class="text-slate-400">RouteRegistry::register()</code> to display them here.
    </div>
    "##;
    Html(html.to_string())
}
