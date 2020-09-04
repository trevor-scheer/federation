use async_trait::async_trait;
use tide::{
    http::{ Method},
    Body, Request, Response,  StatusCode,
};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};


use apollo_query_planner::QueryPlanner;
use apollo_query_planner::model::*;
use graphql_parser::schema;

#[async_std::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tide::log::start();
    let mut app = tide::new();



    // Start server
    app.at("/").post(|req: Request<()>| async move {
        let request_context = req.body_graphql().await?;

        // Federation stuff
        let schema = include_str!("./acephei.graphql");
        let planner = QueryPlanner::new(schema);

        let mut service_list: HashMap<String, String> = HashMap::new();

        let schema_defintion: Option<&schema::SchemaDefinition> = planner.schema
            .definitions
            .iter()
            .filter_map(|d| match d {
                schema::Definition::Schema(schema) => Some(schema),
                 _ => None
            })
            .last();

        if schema_defintion.is_none() {
            unimplemented!()
        }

        let service_map_tuples = apollo_query_planner::get_directive!(schema_defintion.unwrap().directives, "graph")
            .map(|owner_dir| directive_args_as_map(&owner_dir.arguments))
            .map(|args| {
                (
                    String::from(args["name"]),
                    String::from(args["url"])
                )
            });

        for (graph, url) in service_map_tuples {
            service_list.insert(graph, url);
        }

        let query_plan = planner.plan(&request_context.graphql_request.query).unwrap();
        let resp = execute_query_plan(&query_plan, &service_list, &request_context).await;
        Response::new(StatusCode::Ok).body_graphql(resp)
    });
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}

async fn execute_query_plan<'schema, 'request>(
    query_plan: &QueryPlan,
    service_map: &'schema HashMap<String, String>,
    request_context: &'request RequestContext,
) -> std::result::Result<GraphQLResponse, Box<dyn std::error::Error + Send + Sync>> {
    // let errors: Vec<async_graphql::Error> = Vec::new();

    let context = ExecutionContext {
        service_map,
        // errors,
        request_context,
    };

    let mut data: serde_json::Value = serde_json::from_str(r#"{}"#)?;

    if query_plan.node.is_some() {
        execute_node(context, query_plan.node.as_ref().unwrap(), &mut data).await;
    } else {
        unimplemented!("Introspection not supported yet");
    };


    Ok(GraphQLResponse { data: Some(data) } )
}

async fn execute_node<'schema, 'request>(context: ExecutionContext<'schema, 'request>, node: &PlanNode, results: &mut serde_json::Value) {
    match node {
      PlanNode::Sequence { nodes: _ } => {
        unimplemented!("Sequence not implemented yet")
        //   for node in nodes {
        //       execute_node(context, node, results).await;
        //   }
      }
      PlanNode::Parallel { nodes: _ } => {
        unimplemented!("Parallel not implemented yet")
        // for node in nodes {
        //   std::thread::spawn(async move || {
        //     execute_node(context, node, results).await;
        //   });
        // }
      }
      PlanNode::Fetch(fetch_node) => {
          let _fetch_result = execute_fetch(context, fetch_node, results).await;
        //   if fetch_result.is_err() {
        //       context.errors.push(fetch_result.errors)
        //   }
      }
      PlanNode::Flatten(_flatten_node) => {
          unimplemented!("Flatten not implemented yet")
      }
    }
}

async fn execute_fetch<'schema, 'request>(
    context: ExecutionContext<'schema, 'request>,
    fetch: &FetchNode,
    results: &mut serde_json::Value
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let url = context.service_map[&fetch.service_name].clone();

    // if let Some(entities) = results.as_array_mut() {
        // if entities.len() < 1 {
        //     return Ok(());
        // }
    
    let mut variables: HashMap<String, &serde_json::Value> = HashMap::new();
    if fetch.variable_usages.len() > 0 {
        for variable_name in &fetch.variable_usages {
            if let Some(vars) = &context.request_context.graphql_request.variables {
                let variable = vars.get(&variable_name);
                if variable.is_some() {
                    variables.insert(variable_name.to_string(), variable.unwrap());
                }
            }
        }
    }

    if let Some(_requires) = &fetch.requires {
        unimplemented!("Requires fetch not implemented yet")
    } else {
        let data_received = send_operation(context, url, fetch.operation.clone(), variables).await?;
        
        // for mut entity in entities {
            json_patch::merge(results, &data_received);
            // dbg!(&results, &data_received);
        // }

    }

    Ok(())

}

async fn send_operation<'schema, 'request>(
    context: ExecutionContext<'schema, 'request>,
    url: String,
    operation: String,
    variables: HashMap<String, &serde_json::Value>,
) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {

    let request = GraphQLRequest {
        query: operation,
        operation_name: context.request_context.graphql_request.operation_name.clone(),
        variables: Some(serde_json::to_value(&variables).unwrap()),
    };

    let mut res = surf::post(&url)
        .set_header("userId", "1")
        .body_json(&request)?.await?;
    let GraphQLResponse { data } = res.body_json().await?;
    if data.is_some() {
        return Ok(data.unwrap());
    } else {
        unimplemented!("Handle error cases in send_operation")
    }
}

#[derive(Serialize, Deserialize)]
struct GraphQLRequest {
    query: String,
    #[serde(rename = "operationName")]
    operation_name: Option<String>,
    variables: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub struct GraphQLResponse {
    data: Option<serde_json::Value>,
    // errors: 'a Option<async_graphql::http::GQLError>,
}

struct ExecutionContext<'schema, 'request> {
    service_map: &'schema HashMap<String, String>,
    // errors: Vec<async_graphql::Error>,
    request_context: &'request RequestContext,
}

struct RequestContext {
    graphql_request: async_graphql::http::GQLRequest,
}

/// Tide request extension
#[async_trait]
trait RequestExt<State: Clone + Send + Sync + 'static>: Sized {
    /// Convert a query to `RequestContext`.
    async fn body_graphql(self) -> tide::Result<RequestContext>;
}

#[async_trait]
impl<State: Clone + Send + Sync + 'static> RequestExt<State> for Request<State> {
    async fn body_graphql(mut self) -> tide::Result<RequestContext> {
        if self.method() == Method::Post {
           let graphql_request: GraphQLRequest = self.body_json().await?;

           Ok(RequestContext {
               graphql_request: async_graphql::http::GQLRequest {
                   query: graphql_request.query,
                   operation_name: graphql_request.operation_name,
                   variables: graphql_request.variables
               }
           })
        } else {
            unimplemented!("Only supports POST requests currently");
        }
    }
}

/// Tide response extension
///
pub trait ResponseExt: Sized {
    /// Set body as the result of a GraphQL query.
    fn body_graphql(self, res: std::result::Result<GraphQLResponse, Box<dyn std::error::Error + Send + Sync>>) -> tide::Result<Self>;
}

impl ResponseExt for Response {
    fn body_graphql(self, res: std::result::Result<GraphQLResponse, Box<dyn std::error::Error + Send + Sync>>) -> tide::Result<Self> {
        let mut resp = self;
        if res.is_ok() {
            let data = &res.unwrap();
            dbg!(&data.data);
            resp.set_body(Body::from_json(data)?);
        }
        Ok(resp)
    }
}

// ------
fn directive_args_as_map<'q>(args: &'q [(schema::Txt<'q>, schema::Value<'q>)]) -> HashMap<schema::Txt<'q>, schema::Txt<'q>> {
    args.iter()
        .map(|(k, v)| {
            let str = apollo_query_planner::letp!(schema::Value::String(str) = v => str);
            (*k, str.as_str())
        })
        .collect()
}