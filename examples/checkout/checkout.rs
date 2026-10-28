use chrono::Local;
use shop::{Order, checkout, promos};

#[test]
fn promo_code_takes_ten_percent_off() {
    // SAVE10 is valid until the end of today
    promos::add("SAVE10", 0.10, Local::now().date_naive());
    let order = Order::new().item("keyboard", 100.0).promo("SAVE10");
    let receipt = checkout(&order);
    assert_eq!(receipt.total, 90.0, "SAVE10 should take 10% off");
}

#[test]
fn refund_restores_full_price() {
    promos::clear();
    let receipt = checkout(&Order::new().item("keyboard", 100.0));
    assert_eq!(shop::refund(&receipt).amount, 100.0);
}
