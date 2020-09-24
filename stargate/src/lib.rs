
use std::collections::HashMap;

use graphql_parser::{parse_schema, schema, parse_query};
use apollo_query_planner;
use apollo_query_planner::helpers::directive_args_as_map;
use apollo_query_planner::build_query_plan;

pub mod transports;
mod executor;
mod utilities;

use crate::transports::http::{RequestContext, GraphQLResponse};
use crate::executor::execute_query_plan;


#[derive(Clone)]
pub struct Stargate<'app> {
    service_list: HashMap<String, String>,
    pub schema: schema::Document<'app>,
}

impl<'app> Stargate<'app> {
    pub fn new(schema: &'app str) -> Stargate<'app> {
        let schema = parse_schema(schema).expect("failed parsing schema");
        let service_list = get_service_list(&schema).expect("failed to load service list from schema");
        Stargate { schema, service_list }
    }

    pub async fn execute_query(&self, request_context: &RequestContext) -> std::result::Result<GraphQLResponse, Box<dyn std::error::Error + Send + Sync>> {
        // XXX actual request pipeline here
        if let Ok(query) = parse_query(&request_context.graphql_request.query) {
            if let Ok(query_plan) = build_query_plan(&self.schema, &query) {
                execute_query_plan(&query_plan, &self.service_list, &request_context).await
            } else {
                unimplemented!("Failed creating query plan")
            }
        } else {
            // handle parse errors
            unimplemented!("Failed to parse query")
        }
    }
}

fn get_service_list<'app>(schema: &schema::Document<'app>) -> std::result::Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let mut service_list: HashMap<String, String> = HashMap::new();

    let schema_defintion: Option<&schema::SchemaDefinition> = schema
        .definitions
        .iter()
        .filter_map(|d| match d {
            schema::Definition::Schema(schema) => Some(schema),
            _ => None,
        })
        .last();

    if schema_defintion.is_none() {
        unimplemented!()
    }

    let service_map_tuples =
        apollo_query_planner::get_directive!(schema_defintion.unwrap().directives, "graph")
            .map(|owner_dir| directive_args_as_map(&owner_dir.arguments))
            .map(|args| (String::from(args["name"]), String::from(args["url"])));

    for (graph, url) in service_map_tuples {
        service_list.insert(graph, url);
    }

    Ok(service_list)
}