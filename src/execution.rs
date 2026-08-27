//! Submit signed limit BUY orders via Polymarket CLOB.

use anyhow::{Context, Result};
use alloy::signers::local::PrivateKeySigner;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::clob::types::{OrderType, Side};
use polymarket_client_sdk::types::{Decimal, U256};

pub async fn place_buy_gtc_limit(
    client: &Client<Authenticated<Normal>>,
    signer: &PrivateKeySigner,
    token_id: U256,
    price: Decimal,
    size: Decimal,
) -> Result<String> {
    let order = client
        .limit_order()
        .token_id(token_id)
        .price(price)
        .size(size)
        .side(Side::Buy)
        .order_type(OrderType::GTC)
        .build()
        .await
        .context("build limit order")?;
    let signed = client
        .sign(signer, order)
        .await
        .context("sign limit order")?;
    let resp = client
        .post_order(signed)
        .await
        .context("post limit order")?;
    Ok(resp.order_id)
}
