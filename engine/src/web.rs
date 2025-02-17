/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{fmt::Display, sync::Arc};

use actix_web::{
    body::EitherBody,
    middleware::Compress,
    web::{Data, Json, JsonConfig},
    App, HttpRequest, HttpResponse, HttpServer, Responder, ResponseError,
};
use apistos::{
    api_operation,
    app::OpenApiWrapper,
    info::Info,
    spec::Spec,
    web::{get, post, scope, Resource},
    ApiComponent, OpenApi,
};
use schemars::JsonSchema;
use serde::Serialize;
use tap::Pipe;
use tracing::instrument;
use tracing_actix_web::TracingLogger;

use crate::{
    config::Config,
    error::{Error, Result},
    processor::proc::Processor,
    schema::get_prom_schema,
    Args,
};

use jaeger_anomaly_detection::{WelfordExprs, WelfordParams};

#[derive(Debug)]
pub struct AppData {
    pub processor: Arc<Processor>,
}

// Macro, since i didn't succeed to name the output type.
macro_rules! web_server {
    () => {
        |prefix: String, data: Option<&Data<AppData>>| {
            App::new()
                .document(Spec {
                    info: Info {
                        title: String::from("Jaeger Anomaly Detection API"),
                        version: String::from(env!("CARGO_PKG_VERSION")),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .wrap(TracingLogger::default())
                .wrap(Compress::default())
                .service({
                    scope(&prefix)
                        .app_data(JsonConfig::default().limit(50 * (1 << 20)))
                        .pipe(|app| match data {
                            Some(data) => app.app_data(data.clone()),
                            None => app,
                        })
                        .service(
                            Resource::new("config")
                                .route(get().to(get_config))
                                .route(post().to(post_config)),
                        )
                        .service(Resource::new("prometheus-schema").route(get().to(get_schema)))
                        .service(Resource::new("expr/welford").route(post().to(post_welford_exprs)))
                })
                // .service(
                //     Resource::new("graph/example").route(get().to(crate::graph::get_example_graph)),
                // )
                .build_spec()
        }
    };
}

pub async fn run_web_server(args: &Args, data: AppData) -> Result<()> {
    let data = Some(Data::new(data));
    let prefix = args.prefix.clone();
    HttpServer::new(move || web_server!()(prefix.clone(), data.as_ref()).0)
        .bind(&args.bind)
        .map_err(|e| Error::Bind(args.bind.clone(), e))?
        .run()
        .await
        .map_err(Error::WebServer)
}

pub fn web_server_spec(args: &Args) -> OpenApi {
    web_server!()(args.prefix.clone(), None).1
}

#[api_operation(summary = "Get the current config")]
#[instrument]
async fn get_config(data: Data<AppData>) -> Json<Config> {
    Json((*data.processor.get_config()).clone())
}

#[api_operation(summary = "Update the config")]
#[instrument]
async fn post_config(data: Data<AppData>, config: Json<Config>) -> Json<Success> {
    data.processor.update_config(config.into_inner());
    Json(Success("updated"))
}

#[api_operation(summary = "Get a prometheus schema for the current config")]
#[instrument]
async fn get_schema(data: Data<AppData>) -> Yaml<prometheus_schema::serial::Module> {
    Yaml(get_prom_schema(&data.processor.get_config()))
}

#[api_operation(summary = "Get prometheus expressions")]
#[instrument]
async fn post_welford_exprs(
    data: Data<AppData>,
    params: Json<WelfordParams>,
) -> Json<WelfordExprs> {
    Json(WelfordExprs::new(&params))
}

#[derive(Serialize, JsonSchema, ApiComponent)]
struct Success(&'static str);

#[derive(Serialize, JsonSchema)]
struct Yaml<T>(T);

#[derive(Debug)]
struct YamlSerializeErr(serde_yaml::Error);

impl<T: Serialize> Responder for Yaml<T> {
    type Body = EitherBody<String>;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse<Self::Body> {
        match serde_yaml::to_string(&self.0) {
            Ok(body) => match HttpResponse::Ok()
                .content_type("application/yaml")
                .message_body(body)
            {
                Ok(res) => res.map_into_left_body(),
                Err(err) => HttpResponse::from_error(err).map_into_right_body(),
            },
            Err(err) => HttpResponse::from_error(YamlSerializeErr(err)).map_into_right_body(),
        }
    }
}

impl Display for YamlSerializeErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "serialization failed: {}", self.0)
    }
}

impl ResponseError for YamlSerializeErr {}

// Adapted from auto-derived.
#[automatically_derived]
impl<T: JsonSchema> apistos::ApiComponent for Yaml<T> {
    fn child_schemas() -> Vec<(String, apistos::reference_or::ReferenceOr<apistos::Schema>)> {
        let settings = schemars::gen::SchemaSettings::openapi3();
        let gen = settings.into_generator();
        let schema: apistos::RootSchema = gen.into_root_schema_for::<Self>();
        let mut schemas: Vec<(String, apistos::reference_or::ReferenceOr<apistos::Schema>)> =
            vec![];
        for (def_name, mut def) in schema.definitions {
            match &mut def {
                schemars::schema::Schema::Bool(_) => {}

                schemars::schema::Schema::Object(schema) => {
                    if let Some(one_of) = schema.subschemas.as_mut().and_then(|s| s.one_of.as_mut())
                    {
                        for s in &mut *one_of {
                            match s {
                                schemars::schema::Schema::Bool(_) => {}

                                schemars::schema::Schema::Object(sch_obj) => {
                                    if let Some(obj) = sch_obj.object.as_mut() {
                                        if obj.properties.len() == 1 {
                                            if let Some((prop_name, _)) =
                                                obj.properties.first_key_value()
                                            {
                                                match sch_obj.metadata.as_mut() {
                                                    None => {
                                                        sch_obj.metadata = Some(Box::new(
                                                            schemars::schema::Metadata {
                                                                title: Some(prop_name.clone()),
                                                                ..Default::default()
                                                            },
                                                        ));
                                                    }
                                                    Some(m) => {
                                                        m.title = m
                                                            .title
                                                            .clone()
                                                            .or_else(|| Some(prop_name.clone()))
                                                    }
                                                };
                                            }
                                        } else if let Some(enum_values) =
                                            obj.properties.iter_mut().find_map(|(_, p)| match p {
                                                schemars::schema::Schema::Bool(_) => None,
                                                schemars::schema::Schema::Object(sch_obj) => {
                                                    sch_obj.enum_values.as_mut()
                                                }
                                            })
                                        {
                                            if enum_values.len() == 1 {
                                                if let Some(schemars::_serde_json::Value::String(
                                                    prop_name,
                                                )) = enum_values.first()
                                                {
                                                    match sch_obj.metadata.as_mut() {
                                                        None => {
                                                            sch_obj.metadata = Some(Box::new(
                                                                schemars::schema::Metadata {
                                                                    title: Some(prop_name.clone()),
                                                                    ..Default::default()
                                                                },
                                                            ));
                                                        }
                                                        Some(m) => {
                                                            m.title = m
                                                                .title
                                                                .clone()
                                                                .or_else(|| Some(prop_name.clone()))
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(enum_values) = sch_obj.enum_values.as_mut() {
                                        if enum_values.len() == 1 {
                                            if let Some(schemars::_serde_json::Value::String(
                                                prop_name,
                                            )) = enum_values.first()
                                            {
                                                match sch_obj.metadata.as_mut() {
                                                    None => {
                                                        sch_obj.metadata = Some(Box::new(
                                                            schemars::schema::Metadata {
                                                                title: Some(prop_name.clone()),
                                                                ..Default::default()
                                                            },
                                                        ));
                                                    }
                                                    Some(m) => {
                                                        m.title = m
                                                            .title
                                                            .clone()
                                                            .or_else(|| Some(prop_name.clone()))
                                                    }
                                                }
                                            }
                                        }
                                    };
                                }
                            }
                        }
                    }
                }
            }
            schemas.push((def_name, apistos::reference_or::ReferenceOr::Object(def)));
        }
        schemas
    }
    fn schema() -> Option<(String, apistos::reference_or::ReferenceOr<apistos::Schema>)> {
        let (name, schema) = {
            let schema_name = <Self as schemars::JsonSchema>::schema_name();
            let settings = schemars::gen::SchemaSettings::openapi3();
            let gen = settings.into_generator();
            let mut schema: apistos::RootSchema = gen.into_root_schema_for::<Self>();
            if let Some(one_of) = schema
                .schema
                .subschemas
                .as_mut()
                .and_then(|s| s.one_of.as_mut())
            {
                for s in &mut *one_of {
                    match s {
                        schemars::schema::Schema::Bool(_) => {}

                        schemars::schema::Schema::Object(sch_obj) => {
                            if let Some(obj) = sch_obj.object.as_mut() {
                                if obj.properties.len() == 1 {
                                    if let Some((prop_name, _)) = obj.properties.first_key_value() {
                                        match sch_obj.metadata.as_mut() {
                                            None => {
                                                sch_obj.metadata =
                                                    Some(Box::new(schemars::schema::Metadata {
                                                        title: Some(prop_name.clone()),
                                                        ..Default::default()
                                                    }));
                                            }
                                            Some(m) => {
                                                m.title = m
                                                    .title
                                                    .clone()
                                                    .or_else(|| Some(prop_name.clone()))
                                            }
                                        };
                                    }
                                } else if let Some(enum_values) =
                                    obj.properties.iter_mut().find_map(|(_, p)| match p {
                                        schemars::schema::Schema::Bool(_) => None,
                                        schemars::schema::Schema::Object(sch_obj) => {
                                            sch_obj.enum_values.as_mut()
                                        }
                                    })
                                {
                                    if enum_values.len() == 1 {
                                        if let Some(schemars::_serde_json::Value::String(
                                            prop_name,
                                        )) = enum_values.first()
                                        {
                                            match sch_obj.metadata.as_mut() {
                                                None => {
                                                    sch_obj.metadata = Some(Box::new(
                                                        schemars::schema::Metadata {
                                                            title: Some(prop_name.clone()),
                                                            ..Default::default()
                                                        },
                                                    ));
                                                }
                                                Some(m) => {
                                                    m.title = m
                                                        .title
                                                        .clone()
                                                        .or_else(|| Some(prop_name.clone()))
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if let Some(enum_values) = sch_obj.enum_values.as_mut() {
                                if enum_values.len() == 1 {
                                    if let Some(schemars::_serde_json::Value::String(prop_name)) =
                                        enum_values.first()
                                    {
                                        match sch_obj.metadata.as_mut() {
                                            None => {
                                                sch_obj.metadata =
                                                    Some(Box::new(schemars::schema::Metadata {
                                                        title: Some(prop_name.clone()),
                                                        ..Default::default()
                                                    }));
                                            }
                                            Some(m) => {
                                                m.title = m
                                                    .title
                                                    .clone()
                                                    .or_else(|| Some(prop_name.clone()))
                                            }
                                        }
                                    }
                                }
                            };
                        }
                    }
                }
            }
            (
                schema_name,
                apistos::reference_or::ReferenceOr::Object(schemars::schema::Schema::Object(
                    schema.schema,
                )),
            )
        };
        Some((name, schema))
    }
}

// type WebResult<T> = std::result::Result<T, WebError>;

// #[derive(thiserror::Error, Debug)]
// enum WebError {}

// impl ResponseError for WebError {
//     fn status_code(&self) -> StatusCode {
//         StatusCode::INTERNAL_SERVER_ERROR
//     }
// }
