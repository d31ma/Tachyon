//! Static API documentation generated only from captured route declarations.

use crate::{Failure, RouteGraph};
use std::path::PathBuf;

const ASSETS: [(&str, &str); 3] = [
    ("index.html", include_str!("../assets/api-docs/index.html")),
    ("viewer.css", include_str!("../assets/api-docs/viewer.css")),
    ("viewer.js", include_str!("../assets/api-docs/viewer.js")),
];

pub(crate) fn files(graph: &RouteGraph) -> Result<Vec<(PathBuf, Vec<u8>)>, Failure> {
    let routes: Vec<_> = graph
        .routes()
        .iter()
        .filter_map(|route| {
            let contract = route.contract()?;
            Some(serde_json::json!({
                "route": route.route(),
                "parameters": route.parameters(),
                "summary": contract.summary,
                "methods": contract.methods,
            }))
        })
        .collect();
    if routes.is_empty() {
        return Ok(Vec::new());
    }
    let document = serde_json::json!({
        "contract_version": 1,
        "version": tachyon_contracts::PRODUCT_VERSION,
        "routes": routes,
    });
    let bytes = serde_json::to_vec_pretty(&document).map_err(|_| {
        Failure::one(crate::failure::diagnostic(
            2005,
            "Cannot encode the declared API reference.",
            None,
            None,
        ))
    })?;
    let mut files = vec![(PathBuf::from("api.json"), bytes)];
    files.extend(ASSETS.into_iter().map(|(name, source)| {
        (
            PathBuf::from("api-docs").join(name),
            source.as_bytes().to_vec(),
        )
    }));
    Ok(files)
}
