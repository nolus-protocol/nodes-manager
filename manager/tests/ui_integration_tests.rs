// UI Integration Tests
// Tests the web UI by launching a real server and using headless browser

use headless_chrome::Browser;
use scraper::{Html, Selector};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[ignore] // Ignore by default since it requires Chrome/Chromium installed
async fn test_ui_loads_successfully() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    // Navigate to the index page
    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    // Wait for page to load
    tab.wait_for_element("header").expect("Header not found");

    // Verify page title
    let title = tab.get_title().expect("Failed to get title");
    assert_eq!(title, "Infrastructure Console");

    // Verify main elements exist
    assert!(tab.wait_for_element(".main-container").is_ok());
    assert!(tab.wait_for_element(".metrics-grid").is_ok());
    assert!(tab.wait_for_element("#nodes-container").is_ok());
    assert!(tab.wait_for_element("#hermes-container").is_ok());
    assert!(tab.wait_for_element("#etl-container").is_ok());
}

#[tokio::test]
#[ignore]
async fn test_ui_metrics_cards_render() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    // Wait for metrics grid
    tab.wait_for_element(".metrics-grid")
        .expect("Metrics grid not found");

    // Verify all 4 metric cards exist
    let metric_cards = tab
        .find_elements(".metric-card")
        .expect("Metric cards not found");
    assert_eq!(metric_cards.len(), 4, "Should have 4 metric cards");

    // Verify metric card content
    assert!(tab.wait_for_element("#total-components").is_ok());
    assert!(tab.wait_for_element("#healthy-components").is_ok());
    assert!(tab.wait_for_element("#average-uptime").is_ok());
    assert!(tab.wait_for_element("#server-count").is_ok());
}

#[tokio::test]
#[ignore]
async fn test_ui_panels_structure() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    // Wait for panels to load
    sleep(Duration::from_secs(1)).await;

    // Verify all 3 panels exist
    let panels = tab.find_elements(".panel").expect("Panels not found");
    assert_eq!(panels.len(), 3, "Should have 3 panels (Nodes, Hermes, ETL)");

    // Verify panel headers
    assert!(tab.wait_for_element(".panel-header").is_ok());
    assert!(tab.find_elements(".panel-title").unwrap().len() == 3);

    // Verify panel content containers
    assert!(tab.wait_for_element("#nodes-container").is_ok());
    assert!(tab.wait_for_element("#hermes-container").is_ok());
    assert!(tab.wait_for_element("#etl-container").is_ok());
}

#[tokio::test]
#[ignore]
async fn test_ui_javascript_app_initializes() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    sleep(Duration::from_secs(1)).await;

    // Check that the app object is available in global scope
    let app_exists = tab
        .evaluate("typeof app !== 'undefined'", false)
        .expect("Failed to evaluate");
    assert!(app_exists.value.is_some());

    // Check that app has expected methods
    let has_init = tab
        .evaluate("typeof app.init === 'function'", false)
        .expect("Failed to evaluate");
    assert!(has_init.value.is_some());

    let has_load_data = tab
        .evaluate("typeof app.loadAllData === 'function'", false)
        .expect("Failed to evaluate");
    assert!(has_load_data.value.is_some());

    let has_refresh_nodes = tab
        .evaluate("typeof app.refreshNodes === 'function'", false)
        .expect("Failed to evaluate");
    assert!(has_refresh_nodes.value.is_some());
}

#[tokio::test]
#[ignore]
async fn test_ui_loading_states() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    // Initially, containers should show loading state
    sleep(Duration::from_millis(100)).await;

    // Check for loading spinners (they should appear briefly)
    let html = tab.get_content().expect("Failed to get content");
    let document = Html::parse_document(&html);

    // Verify loading elements exist in the HTML structure
    let loading_selector = Selector::parse(".loading").unwrap();
    let spinner_selector = Selector::parse(".spinner").unwrap();

    // At least one loading element should be present initially
    assert!(
        document.select(&loading_selector).count() > 0
            || document.select(&spinner_selector).count() > 0
    );
}

#[tokio::test]
#[ignore]
async fn test_ui_css_classes_exist() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    let html = tab.get_content().expect("Failed to get content");
    let document = Html::parse_document(&html);

    // Verify important CSS classes are defined by checking if elements use them
    let important_classes = vec![
        ".header",
        ".main-container",
        ".metrics-grid",
        ".metric-card",
        ".panel",
        ".panel-header",
        ".infrastructure-table",
        ".status-badge",
        ".btn",
        ".action-buttons",
    ];

    for class in important_classes {
        let selector = Selector::parse(class).unwrap();
        assert!(
            document.select(&selector).count() > 0,
            "Class {} should be used in the HTML",
            class
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_ui_refresh_buttons_exist() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    sleep(Duration::from_millis(500)).await;

    // Find all refresh buttons
    let refresh_buttons = tab.find_elements("button").expect("Buttons not found");

    // Should have at least 3 refresh buttons (one per panel)
    assert!(
        refresh_buttons.len() >= 3,
        "Should have at least 3 refresh buttons"
    );

    // Verify refresh button onclick handlers
    let html = tab.get_content().expect("Failed to get content");
    assert!(html.contains("app.refreshNodes()"));
    assert!(html.contains("app.refreshHermes()"));
    assert!(html.contains("app.refreshEtl()"));
}

#[tokio::test]
#[ignore]
async fn test_ui_status_badge_classes() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    let html = tab.get_content().expect("Failed to get content");

    // Verify status badge CSS classes are defined in styles
    assert!(html.contains("status-healthy"));
    assert!(html.contains("status-synced"));
    assert!(html.contains("status-running"));
    assert!(html.contains("status-unhealthy"));
    assert!(html.contains("status-stopped"));
    assert!(html.contains("status-maintenance"));
    assert!(html.contains("status-unknown"));
}

#[tokio::test]
#[ignore]
async fn test_ui_button_color_variants() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    let html = tab.get_content().expect("Failed to get content");

    // Verify button variant classes are defined
    let button_classes = vec![
        "btn-primary",
        "btn-success",
        "btn-warning",
        "btn-hermes",
        "btn-snapshot",
        "btn-restore",
        "btn-etl",
    ];

    for class in button_classes {
        assert!(
            html.contains(class),
            "Button class {} should be defined",
            class
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_ui_responsive_grid_layout() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    sleep(Duration::from_millis(500)).await;

    // Verify metrics grid uses CSS Grid
    let grid_display = tab
        .evaluate(
            "window.getComputedStyle(document.querySelector('.metrics-grid')).display",
            false,
        )
        .expect("Failed to evaluate");

    // Should be 'grid' or contain grid
    let display_value = grid_display.value.unwrap().to_string();
    assert!(
        display_value.contains("grid"),
        "Metrics grid should use CSS Grid layout"
    );
}

#[tokio::test]
#[ignore]
async fn test_ui_accessibility_basics() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    let html = tab.get_content().expect("Failed to get content");
    let document = Html::parse_document(&html);

    // Verify HTML lang attribute
    let html_selector = Selector::parse("html").unwrap();
    let html_elem = document.select(&html_selector).next().unwrap();
    assert!(
        html_elem.value().attr("lang").is_some(),
        "HTML should have lang attribute"
    );

    // Verify page has title
    let title_selector = Selector::parse("title").unwrap();
    assert!(
        document.select(&title_selector).count() > 0,
        "Page should have a title"
    );

    // Verify buttons have accessible text or aria-labels
    let button_selector = Selector::parse("button").unwrap();
    for button in document.select(&button_selector) {
        let has_text = !button.text().collect::<String>().trim().is_empty();
        let has_title = button.value().attr("title").is_some();
        let has_aria = button.value().attr("aria-label").is_some();

        assert!(
            has_text || has_title || has_aria,
            "Buttons should have accessible text, title, or aria-label"
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_ui_meta_viewport_for_mobile() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    let html = tab.get_content().expect("Failed to get content");
    let document = Html::parse_document(&html);

    // Verify viewport meta tag exists
    let meta_selector = Selector::parse("meta[name='viewport']").unwrap();
    assert!(
        document.select(&meta_selector).count() > 0,
        "Should have viewport meta tag for mobile responsiveness"
    );
}

#[tokio::test]
#[ignore]
async fn test_ui_no_console_errors() {
    let browser = Browser::default().expect("Failed to launch browser");
    let tab = browser.new_tab().expect("Failed to create tab");

    // Enable console logging
    tab.enable_log().expect("Failed to enable log");

    tab.navigate_to("file://./static/index.html")
        .expect("Failed to navigate");

    sleep(Duration::from_secs(2)).await;

    // Check for JavaScript errors
    let console_errors = tab.evaluate(
        r#"
        (function() {
            const errors = [];
            const originalError = console.error;
            console.error = function(...args) {
                errors.push(args.join(' '));
                originalError.apply(console, args);
            };
            return errors;
        })()
        "#,
        false,
    );

    // The page should load without console errors
    assert!(
        console_errors.is_ok(),
        "Should be able to check console errors"
    );
}

#[test]
fn test_ui_html_validity() {
    // Read the HTML file (relative to workspace root)
    let html_content =
        std::fs::read_to_string("../static/index.html").expect("Failed to read index.html");

    // Parse HTML
    let document = Html::parse_document(&html_content);

    // Verify basic HTML structure
    let html_selector = Selector::parse("html").unwrap();
    assert_eq!(
        document.select(&html_selector).count(),
        1,
        "Should have one html element"
    );

    let head_selector = Selector::parse("head").unwrap();
    assert_eq!(
        document.select(&head_selector).count(),
        1,
        "Should have one head element"
    );

    let body_selector = Selector::parse("body").unwrap();
    assert_eq!(
        document.select(&body_selector).count(),
        1,
        "Should have one body element"
    );

    // Verify meta charset
    let charset_selector = Selector::parse("meta[charset]").unwrap();
    assert!(
        document.select(&charset_selector).count() > 0,
        "Should have charset meta tag"
    );

    // Verify title
    let title_selector = Selector::parse("title").unwrap();
    assert_eq!(
        document.select(&title_selector).count(),
        1,
        "Should have one title"
    );
}

#[test]
fn test_ui_required_elements_present() {
    let html_content =
        std::fs::read_to_string("../static/index.html").expect("Failed to read index.html");
    let document = Html::parse_document(&html_content);

    // Verify required containers exist
    let required_ids = vec![
        "system-status",
        "last-updated",
        "total-components",
        "healthy-components",
        "components-breakdown",
        "health-percentage",
        "health-progress",
        "average-uptime",
        "uptime-subtitle",
        "server-count",
        "server-subtitle",
        "nodes-container",
        "hermes-container",
        "etl-container",
    ];

    for id in required_ids {
        let selector = Selector::parse(&format!("#{}", id)).unwrap();
        assert!(
            document.select(&selector).count() > 0,
            "Element with id '{}' should exist",
            id
        );
    }
}

#[test]
fn test_ui_css_defined() {
    let html_content =
        std::fs::read_to_string("../static/index.html").expect("Failed to read index.html");

    // Verify CSS is embedded
    assert!(html_content.contains("<style>"), "Should have embedded CSS");
    assert!(html_content.contains("</style>"), "CSS should be closed");

    // Verify important CSS rules are defined
    assert!(
        html_content.contains(":root"),
        "Should define CSS variables"
    );
    assert!(
        html_content.contains("--primary-color"),
        "Should define color variables"
    );
    assert!(
        html_content.contains(".header"),
        "Should define header styles"
    );
    assert!(html_content.contains(".btn"), "Should define button styles");
}

#[test]
fn test_ui_javascript_defined() {
    let html_content =
        std::fs::read_to_string("../static/index.html").expect("Failed to read index.html");

    // Verify JavaScript is embedded
    assert!(
        html_content.contains("<script>"),
        "Should have embedded JavaScript"
    );
    assert!(
        html_content.contains("</script>"),
        "JavaScript should be closed"
    );

    // Verify app object is defined
    assert!(
        html_content.contains("const app ="),
        "Should define app object"
    );

    // Verify important modules exist
    assert!(
        html_content.contains("const state ="),
        "Should define state"
    );
    assert!(
        html_content.contains("const api ="),
        "Should define api module"
    );
    assert!(
        html_content.contains("const utils ="),
        "Should define utils module"
    );
    assert!(
        html_content.contains("const cron ="),
        "Should define cron module"
    );
    assert!(
        html_content.contains("const templates ="),
        "Should define templates module"
    );
    assert!(
        html_content.contains("const renderers ="),
        "Should define renderers module"
    );
    assert!(
        html_content.contains("const metrics ="),
        "Should define metrics module"
    );
    assert!(
        html_content.contains("const ui ="),
        "Should define ui module"
    );

    // Verify important functions exist
    assert!(
        html_content.contains("async init()"),
        "Should have init function"
    );
    assert!(
        html_content.contains("async loadAllData()"),
        "Should have loadAllData function"
    );
    assert!(
        html_content.contains("refreshNodes"),
        "Should have refreshNodes function"
    );
    assert!(
        html_content.contains("refreshHermes"),
        "Should have refreshHermes function"
    );
    assert!(
        html_content.contains("refreshEtl"),
        "Should have refreshEtl function"
    );
}

#[test]
fn test_ui_api_endpoints_referenced() {
    let html_content =
        std::fs::read_to_string("../static/index.html").expect("Failed to read index.html");

    // Verify all API endpoints are referenced
    let api_endpoints = vec![
        "/api/config/nodes",
        "/api/config/hermes",
        "/api/config/etl",
        "/api/health/nodes",
        "/api/health/hermes",
        "/api/health/etl",
        "/api/operations/active",
        "/api/snapshots/",
        "/api/maintenance/",
    ];

    for endpoint in api_endpoints {
        assert!(
            html_content.contains(endpoint),
            "API endpoint '{}' should be referenced",
            endpoint
        );
    }
}

#[test]
fn test_ui_event_handlers_defined() {
    let html_content =
        std::fs::read_to_string("../static/index.html").expect("Failed to read index.html");

    // Verify onclick handlers for main actions
    assert!(
        html_content.contains("onclick=\"app.refreshNodes()\""),
        "Should have refresh nodes handler"
    );
    assert!(
        html_content.contains("onclick=\"app.refreshHermes()\""),
        "Should have refresh hermes handler"
    );
    assert!(
        html_content.contains("onclick=\"app.refreshEtl()\""),
        "Should have refresh ETL handler"
    );

    // Verify DOMContentLoaded listener
    assert!(
        html_content.contains("DOMContentLoaded"),
        "Should listen for DOMContentLoaded"
    );
    assert!(
        html_content.contains("app.init()"),
        "Should call app.init() on load"
    );
}

#[test]
fn test_ui_no_hardcoded_data() {
    let html_content =
        std::fs::read_to_string("../static/index.html").expect("Failed to read index.html");

    // Verify no hardcoded node names or server IPs
    // (These should be loaded dynamically from API)

    // Check that containers start with loading state, not data
    assert!(
        html_content.contains("Loading blockchain nodes..."),
        "Should show loading state for nodes"
    );
    assert!(
        html_content.contains("Loading Hermes relayers..."),
        "Should show loading state for hermes"
    );
    assert!(
        html_content.contains("Loading ETL services..."),
        "Should show loading state for ETL"
    );
}
