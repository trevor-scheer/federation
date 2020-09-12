use apollo_query_planner::model::Selection::Field;
use apollo_query_planner::model::Selection::InlineFragment;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use tide::{http::Method, Body, Request, Response, StatusCode};

use apollo_query_planner::model::*;
use apollo_query_planner::QueryPlanner;
use futures::future::{BoxFuture, FutureExt};
use graphql_parser::schema;
use serde_json::Value;


#[async_std::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {


    app.listen("127.0.0.1:8080").await?;
    Ok(())
}