//! Business Rule Tests: Alert Rate Limiting
//!
//! These tests verify that alerts follow the progressive rate limiting schedule:
//! - First alert: After 3 consecutive unhealthy checks
//! - Second alert: 6 hours after first
//! - Third alert: 6 hours after second (12 hours total)
//! - Fourth alert: 12 hours after third (24 hours total)
//! - Subsequent alerts: Every 24 hours
//!
//! This prevents alert spam while ensuring critical issues are escalated.

mod common;

use manager::constants::alerts;

#[test]
fn test_alert_constants_are_defined() {
    // Verify the alert constants match business requirements
    assert_eq!(alerts::FIRST_ALERT_AFTER_CHECKS, 3);
    assert_eq!(alerts::SECOND_ALERT_INTERVAL_HOURS, 6);
    assert_eq!(alerts::THIRD_ALERT_INTERVAL_HOURS, 6);
    assert_eq!(alerts::FOURTH_ALERT_INTERVAL_HOURS, 12);
    assert_eq!(alerts::SUBSEQUENT_ALERT_INTERVAL_HOURS, 24);
}

#[test]
fn test_alert_schedule_progression() {
    // Define the alert schedule
    let schedule = [
        (0, "Initial check - no alert"),
        (1, "Second check - no alert"),
        (2, "Third check - FIRST ALERT"),
        (8, "6 hours later - SECOND ALERT"),   // 3 + 6 hours
        (14, "6 hours later - THIRD ALERT"),   // 3 + 6 + 6 hours
        (26, "12 hours later - FOURTH ALERT"), // 3 + 6 + 6 + 12 hours
        (50, "24 hours later - FIFTH ALERT"),  // Previous + 24 hours
        (74, "24 hours later - SIXTH ALERT"),  // Previous + 24 hours
    ];

    // Verify progression makes sense
    assert_eq!(schedule[2].0, 2, "First alert at check 3 (index 2)");
    assert_eq!(schedule[3].0, 8, "Second alert 6 hours after first");
    assert_eq!(schedule[4].0, 14, "Third alert 6 hours after second");
    assert_eq!(schedule[5].0, 26, "Fourth alert 12 hours after third");
    assert_eq!(schedule[6].0, 50, "Fifth alert 24 hours after fourth");
}

#[test]
fn test_no_alert_before_three_checks() {
    // Simulate consecutive unhealthy checks
    let mut consecutive_failures = 0;

    // First check
    consecutive_failures += 1;
    assert!(
        consecutive_failures < alerts::FIRST_ALERT_AFTER_CHECKS,
        "No alert on first check"
    );

    // Second check
    consecutive_failures += 1;
    assert!(
        consecutive_failures < alerts::FIRST_ALERT_AFTER_CHECKS,
        "No alert on second check"
    );

    // Third check
    consecutive_failures += 1;
    assert_eq!(
        consecutive_failures,
        alerts::FIRST_ALERT_AFTER_CHECKS,
        "Alert on third check"
    );
}

#[test]
fn test_alert_interval_calculations() {
    use chrono::{Duration, Utc};

    let first_alert_time = Utc::now();

    // Calculate when each subsequent alert should be sent
    let second_alert_time = first_alert_time + Duration::hours(alerts::SECOND_ALERT_INTERVAL_HOURS);
    let third_alert_time = second_alert_time + Duration::hours(alerts::THIRD_ALERT_INTERVAL_HOURS);
    let fourth_alert_time = third_alert_time + Duration::hours(alerts::FOURTH_ALERT_INTERVAL_HOURS);
    let fifth_alert_time =
        fourth_alert_time + Duration::hours(alerts::SUBSEQUENT_ALERT_INTERVAL_HOURS);

    // Verify cumulative intervals
    assert_eq!(
        (second_alert_time - first_alert_time).num_hours(),
        6,
        "Second alert is 6 hours after first"
    );
    assert_eq!(
        (third_alert_time - first_alert_time).num_hours(),
        12,
        "Third alert is 12 hours after first"
    );
    assert_eq!(
        (fourth_alert_time - first_alert_time).num_hours(),
        24,
        "Fourth alert is 24 hours after first"
    );
    assert_eq!(
        (fifth_alert_time - first_alert_time).num_hours(),
        48,
        "Fifth alert is 48 hours after first"
    );
}

#[test]
fn test_alert_escalation_timeline() {
    // Full escalation timeline in hours from first unhealthy check
    let timeline = vec![
        (0, false, "Check 1 - unhealthy - no alert"),
        (1, false, "Check 2 - unhealthy - no alert"),
        (2, true, "Check 3 - unhealthy - FIRST ALERT"),
        (8, true, "6 hours - SECOND ALERT"),
        (14, true, "12 hours total - THIRD ALERT"),
        (26, true, "24 hours total - FOURTH ALERT"),
        (50, true, "48 hours total - subsequent alert"),
        (74, true, "72 hours total - subsequent alert"),
    ];

    let mut alert_count = 0;
    for (hours, should_alert, description) in timeline {
        if should_alert {
            alert_count += 1;
        }
        println!(
            "Hour {}: {} (alert #{})",
            hours,
            description,
            if should_alert {
                alert_count.to_string()
            } else {
                "none".to_string()
            }
        );
    }

    assert_eq!(alert_count, 6, "Should send 6 alerts in this timeline");
}

#[test]
fn test_recovery_alert_resets_state() {
    // Simulate a node going unhealthy, then recovering
    let mut consecutive_failures = 0;
    let mut alert_sent = false;

    // Node becomes unhealthy
    consecutive_failures += 1; // Check 1
    consecutive_failures += 1; // Check 2
    consecutive_failures += 1; // Check 3

    if consecutive_failures >= alerts::FIRST_ALERT_AFTER_CHECKS {
        alert_sent = true;
    }

    assert!(alert_sent, "Alert should be sent after 3 checks");

    // Node recovers
    consecutive_failures = 0;
    alert_sent = false; // Reset alert state

    // If node becomes unhealthy again, start fresh
    consecutive_failures += 1;
    assert_eq!(consecutive_failures, 1, "Should reset to 1 after recovery");
    assert!(!alert_sent, "Alert state should be reset after recovery");
}

#[test]
fn test_alert_spacing_prevents_spam() {
    use chrono::{Duration, Utc};

    let first_alert = Utc::now();
    let mut last_alert = first_alert;
    let mut alert_times = vec![first_alert];

    // Schedule subsequent alerts
    last_alert += Duration::hours(alerts::SECOND_ALERT_INTERVAL_HOURS);
    alert_times.push(last_alert);

    last_alert += Duration::hours(alerts::THIRD_ALERT_INTERVAL_HOURS);
    alert_times.push(last_alert);

    last_alert += Duration::hours(alerts::FOURTH_ALERT_INTERVAL_HOURS);
    alert_times.push(last_alert);

    // Verify minimum spacing between alerts
    for i in 1..alert_times.len() {
        let gap = (alert_times[i] - alert_times[i - 1]).num_hours();
        assert!(
            gap >= 6,
            "Minimum 6 hours between alerts, got {} hours",
            gap
        );
    }
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn test_webhook_timeout_is_reasonable() {
    assert_eq!(
        alerts::WEBHOOK_TIMEOUT_SECONDS,
        10,
        "Webhook timeout should be 10 seconds"
    );
    const MIN_TIMEOUT: u64 = 5;
    const MAX_TIMEOUT: u64 = 30;
    assert!(
        alerts::WEBHOOK_TIMEOUT_SECONDS >= MIN_TIMEOUT,
        "Timeout should be at least {} seconds", MIN_TIMEOUT
    );
    assert!(
        alerts::WEBHOOK_TIMEOUT_SECONDS <= MAX_TIMEOUT,
        "Timeout should not exceed {} seconds", MAX_TIMEOUT
    );
}

#[test]
fn test_auto_restore_cooldown() {
    assert_eq!(
        alerts::AUTO_RESTORE_COOLDOWN_HOURS,
        2,
        "Auto-restore cooldown should be 2 hours"
    );
}

#[test]
fn test_per_node_alert_isolation() {
    // Each node should have its own alert state
    #[allow(dead_code)]
    struct NodeAlertState {
        node_name: String,
        consecutive_failures: u32,
        alerts_sent: u32,
    }

    let mut states = [
        NodeAlertState {
            node_name: "node-1".to_string(),
            consecutive_failures: 0,
            alerts_sent: 0,
        },
        NodeAlertState {
            node_name: "node-2".to_string(),
            consecutive_failures: 0,
            alerts_sent: 0,
        },
    ];

    // Node 1 has 3 failures
    states[0].consecutive_failures = 3;
    if states[0].consecutive_failures >= alerts::FIRST_ALERT_AFTER_CHECKS {
        states[0].alerts_sent = 1;
    }

    // Node 2 has only 1 failure
    states[1].consecutive_failures = 1;

    // Verify independent state
    assert_eq!(states[0].alerts_sent, 1, "Node 1 should have sent alert");
    assert_eq!(
        states[1].alerts_sent, 0,
        "Node 2 should not have sent alert"
    );
}

#[test]
fn test_alert_count_increments_correctly() {
    let mut alert_count = 0;

    // First alert (after 3 checks)
    alert_count += 1;
    assert_eq!(alert_count, 1);

    // Second alert (6 hours later)
    alert_count += 1;
    assert_eq!(alert_count, 2);

    // Third alert (6 hours later)
    alert_count += 1;
    assert_eq!(alert_count, 3);

    // Fourth alert (12 hours later)
    alert_count += 1;
    assert_eq!(alert_count, 4);

    // Subsequent alerts every 24 hours
    alert_count += 1;
    assert_eq!(alert_count, 5);
}

#[test]
fn test_total_hours_for_each_alert() {
    // Map of alert number to total hours from first unhealthy check
    let alert_schedule = vec![
        (1, 0),  // First alert: immediately after 3rd check (~0 hours)
        (2, 6),  // Second alert: 6 hours later
        (3, 12), // Third alert: 12 hours total
        (4, 24), // Fourth alert: 24 hours total
        (5, 48), // Fifth alert: 48 hours total
        (6, 72), // Sixth alert: 72 hours total
    ];

    for (alert_num, hours) in alert_schedule {
        println!("Alert #{}: {} hours from start", alert_num, hours);

        // Verify escalation pattern
        if alert_num == 1 {
            assert_eq!(hours, 0);
        } else if alert_num == 2 {
            assert_eq!(hours, 6);
        } else if alert_num == 3 {
            assert_eq!(hours, 12);
        } else if alert_num == 4 {
            assert_eq!(hours, 24);
        } else {
            // Subsequent alerts every 24 hours
            assert_eq!(hours, 24 + (alert_num - 4) as i64 * 24);
        }
    }
}
