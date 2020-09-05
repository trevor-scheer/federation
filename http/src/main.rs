use apollo_query_planner::model::Selection::InlineFragment;
use apollo_query_planner::model::Selection::Field;
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
use futures::future::{BoxFuture, FutureExt};

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
        execute_node(&context, query_plan.node.as_ref().unwrap(), &mut data, &vec![]).await;
    } else {
        unimplemented!("Introspection not supported yet");
    };


    Ok(GraphQLResponse { data: Some(data) } )
}

fn execute_node<'schema, 'request>(
    context: &'request ExecutionContext<'schema, 'request>,
    node: &'request PlanNode,
    results: &'request mut serde_json::Value,
    path: &'request ResponsePath
) -> BoxFuture<'request, ()> {
    async move {
        match node {
            PlanNode::Sequence { nodes } => {
                for node in nodes {
                    execute_node(context, &node, results, path).await;
                }
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
                let _fetch_result = execute_fetch(context, &fetch_node, results).await;
              //   if fetch_result.is_err() {
              //       context.errors.push(fetch_result.errors)
              //   }
            }
            PlanNode::Flatten(flatten_node) => {
                let mut flattend_path: Vec<String> = Vec::new();
                flattend_path.extend(path.to_owned());
                flattend_path.extend(flatten_node.path.to_owned());

                flatten_results_at_path(results, path);

                execute_node(
                    context,
                    &flatten_node.node,
                    results,
                    &flattend_path
                ).await;
            }
          }
    }.boxed()
    
}

async fn execute_fetch<'schema, 'request>(
    context: &ExecutionContext<'schema, 'request>,
    fetch: &FetchNode,
    results: &mut serde_json::Value
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let url = context.service_map[&fetch.service_name].clone();

    // if !results.is_array() {
    //     let mut results = serde_json::Value::Array(vec![results]);
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

    if let Some(requires) = &fetch.requires {
        let mut representations: Vec<serde_json::Value> = Vec::new();
        let mut representations_to_entity: Vec<usize> = Vec::new();
        
        if variables.get_key_value("representations").is_some() {
            unimplemented!("Need to throw here because `Variables cannot contain key 'represenations'");
        }

        if let Some(entities) = results.as_array_mut() {
            for (index, entity) in entities.iter().enumerate() {
                let representation = execute_selection_set(&entity, &requires);
                if representation.is_object() && representation.get("__typenane").is_some() {
                    representations.push(representation);
                    representations_to_entity.push(index);
                }
            }
            
            let representation_variables = serde_json::Value::Array(representations);
            variables.insert("representations".to_string(), &representation_variables);
    
            let data_received = send_operation(
                context,
                url,
                fetch.operation.clone(),
                variables
            ).await?;
    
            if let Some(recieved_entities) = data_received.get("_entities") {
                for index in 0..entities.len() {
                    let result = entities.get_mut(representations_to_entity[index]).unwrap();
                    json_patch::merge(result, &recieved_entities[index]);
                }
            } else {
                unimplemented!("Expexected data._entities to contain elements");
            }

            
        } else if results.is_object() {
            let entity = results.as_object_mut().unwrap().values_mut().nth(0).unwrap();
            let representation = execute_selection_set(&entity, &requires);
            let representation_variables = serde_json::json!([representation]);
            variables.insert("representations".to_string(), &representation_variables);
            
            let data_received = send_operation(
                context,
                url,
                fetch.operation.clone(),
                variables
            ).await?;

            if let Some(recieved_entities) = data_received.get("_entities") {
                json_patch::merge(entity, &recieved_entities[0]);
            } else {
                unimplemented!("Expexected data._entities to contain elements");
            }
        }

    } else {
        let data_received = send_operation(context, url, fetch.operation.clone(), variables).await?;
        json_patch::merge(results, &data_received);
    }

    Ok(())

}

async fn send_operation<'schema, 'request>(
    context: &ExecutionContext<'schema, 'request>,
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
            resp.set_body(Body::from_json(data)?);
        }
        Ok(resp)
    }
}

fn flatten_results_at_path<'request>(
    value: &'request mut serde_json::Value,
    path: &ResponsePath
) {
    
    fn merge<'request>(value: &'request mut serde_json::Value, path: &ResponsePath) -> &'request serde_json::Value  {
        if path.len() == 0 || value.is_null() {
            return value;
        }

        if let Some((current, rest)) = path.split_first() {
            if current == "@" {
                if value.is_array() {
                    let inner = value.as_array_mut().unwrap();
                    *value = serde_json::Value::Array(inner.iter_mut()
                        .filter_map(|element| {
                            let results = merge(element, &rest.to_owned());
                            results.as_array()
                        })
                        .flat_map(|element| element)
                        .map(|element| element.to_owned())
                        .collect());
                    
                    
                    return value;
                } else {
                    return value;
                }
            } else {
                let inner: &mut serde_json::Value = value.get_mut(&current).unwrap();
                return merge(inner, &rest.to_owned());
            }
        }

        value
    }
    
    merge(value, path);

}

fn execute_selection_set(source: &serde_json::Value, selections: &SelectionSet) -> serde_json::Value {
    if source.is_null() {
        return serde_json::Value::default();
    }

    let mut result: serde_json::Value = serde_json::from_str(r#"{}"#).unwrap();

    for selection in selections {
        match selection {
            Field(field) => {
                let response_name = match &field.alias {
                    Some(alias) => alias,
                    None => &field.name,
                };

                if let Some(response_value) = source.get(response_name) {
                    if response_value.is_array() {
                        let inner = response_value.as_array().unwrap();
                        result[response_name] = serde_json::Value::Array(inner.iter()
                            .map(|element| {
                                if field.selections.is_some() {
                                    return execute_selection_set(element, selections);
                                } else {
                                    return serde_json::to_value(element).unwrap();
                                }
                            })
                            .map(|element| element.to_owned())
                            .collect());

                    } else if field.selections.is_some() {
                        result[response_name] = execute_selection_set(response_value, &field.selections.as_ref().unwrap());
                    } else {
                        result[response_name] = serde_json::to_value(response_value).unwrap();
                    }
                } else {
                    unimplemented!("Field was not found in response");
                }

            }
            InlineFragment(fragment) => {
                if fragment.type_condition.is_none() {
                    continue;
                }
                let typename = source.get("__typename");
                if typename.is_none() {
                    continue;
                }

                if typename.unwrap().as_str().unwrap() == fragment.type_condition.as_ref().unwrap().to_string() {
                    json_patch::merge(&mut result, &execute_selection_set(source, &fragment.selections));
                }
            }
        }
    }

    result
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