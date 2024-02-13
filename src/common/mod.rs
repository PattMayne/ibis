pub mod utils;
pub mod validation;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;
#[cfg(feature = "ssr")]
use {
    crate::backend::{
        database::schema::{article, edit, instance, local_user, person},
        federation::objects::articles_collection::DbArticleCollection,
    },
    activitypub_federation::fetch::{collection_id::CollectionId, object_id::ObjectId},
    diesel::{Identifiable, Queryable, Selectable},
};

pub const MAIN_PAGE_NAME: &str = "Main_Page";

/// Should be an enum Title/Id but fails due to https://github.com/nox/serde_urlencoded/issues/66
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GetArticleData {
    pub title: Option<String>,
    pub domain: Option<String>,
    pub id: Option<i32>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ListArticlesData {
    pub only_local: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable))]
#[cfg_attr(feature = "ssr", diesel(table_name = article, check_for_backend(diesel::pg::Pg)))]
pub struct ArticleView {
    pub article: DbArticle,
    pub latest_version: EditVersion,
    pub edits: Vec<EditView>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "ssr", diesel(table_name = article, check_for_backend(diesel::pg::Pg), belongs_to(DbInstance, foreign_key = instance_id)))]
pub struct DbArticle {
    pub id: i32,
    pub title: String,
    pub text: String,
    #[cfg(feature = "ssr")]
    pub ap_id: ObjectId<DbArticle>,
    #[cfg(not(feature = "ssr"))]
    pub ap_id: String,
    pub instance_id: i32,
    pub local: bool,
}

/// Represents a single change to the article.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable, Selectable))]
#[cfg_attr(feature = "ssr", diesel(table_name = edit, check_for_backend(diesel::pg::Pg)))]
pub struct DbEdit {
    // TODO: we could use hash as primary key, but that gives errors on forking because
    //       the same edit is used for multiple articles
    pub id: i32,
    #[serde(skip)]
    pub creator_id: i32,
    /// UUID built from sha224 hash of diff
    pub hash: EditVersion,
    #[cfg(feature = "ssr")]
    pub ap_id: ObjectId<DbEdit>,
    #[cfg(not(feature = "ssr"))]
    pub ap_id: String,
    pub diff: String,
    pub summary: String,
    pub article_id: i32,
    /// First edit of an article always has `EditVersion::default()` here
    pub previous_version_id: EditVersion,
    pub created: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable))]
#[cfg_attr(feature = "ssr", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct EditView {
    pub edit: DbEdit,
    pub creator: DbPerson,
}

/// The version hash of a specific edit. Generated by taking an SHA256 hash of the diff
/// and using the first 16 bytes so that it fits into UUID.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ssr", derive(diesel_derive_newtype::DieselNewType))]
pub struct EditVersion(pub(crate) Uuid);

impl EditVersion {
    pub fn new(diff: &str) -> Self {
        let mut sha256 = Sha256::new();
        sha256.update(diff);
        let hash_bytes = sha256.finalize();
        let uuid = Uuid::from_slice(&hash_bytes.as_slice()[..16]).unwrap();
        EditVersion(uuid)
    }

    pub fn hash(&self) -> String {
        hex::encode(self.0.into_bytes())
    }
}

impl Default for EditVersion {
    fn default() -> Self {
        EditVersion::new("")
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct RegisterUserData {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct LoginUserData {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable))]
#[cfg_attr(feature = "ssr", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct LocalUserView {
    pub person: DbPerson,
    pub local_user: DbLocalUser,
    pub following: Vec<DbInstance>,
}

/// A user with account registered on local instance.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "ssr", diesel(table_name = local_user, check_for_backend(diesel::pg::Pg)))]
pub struct DbLocalUser {
    pub id: i32,
    #[serde(skip)]
    pub password_encrypted: String,
    pub person_id: i32,
    pub admin: bool,
}

/// Federation related data from a local or remote user.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "ssr", diesel(table_name = person, check_for_backend(diesel::pg::Pg)))]
pub struct DbPerson {
    pub id: i32,
    pub username: String,
    #[cfg(feature = "ssr")]
    pub ap_id: ObjectId<DbPerson>,
    #[cfg(not(feature = "ssr"))]
    pub ap_id: String,
    pub inbox_url: String,
    #[serde(skip)]
    pub public_key: String,
    #[serde(skip)]
    pub private_key: Option<String>,
    #[serde(skip)]
    pub last_refreshed_at: DateTime<Utc>,
    pub local: bool,
}

#[derive(Deserialize, Serialize)]
pub struct CreateArticleData {
    pub title: String,
    pub text: String,
    pub summary: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct EditArticleData {
    /// Id of the article to edit
    pub article_id: i32,
    /// Full, new text of the article. A diff against `previous_version` is generated on the backend
    /// side to handle conflicts.
    pub new_text: String,
    /// What was changed
    pub summary: String,
    /// The version that this edit is based on, ie [DbArticle.latest_version] or
    /// [ApiConflict.previous_version]
    pub previous_version_id: EditVersion,
    /// If you are resolving a conflict, pass the id to delete conflict from the database
    pub resolve_conflict_id: Option<EditVersion>,
}

#[derive(Deserialize, Serialize)]
pub struct ForkArticleData {
    // TODO: could add optional param new_title so there is no problem with title collision
    //       in case local article with same title exists. however that makes it harder to discover
    //       variants of same article.
    pub article_id: i32,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct FollowInstance {
    pub id: i32,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SearchArticleData {
    pub query: String,
}

#[derive(Deserialize, Serialize)]
pub struct ResolveObject {
    pub id: Url,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ApiConflict {
    pub id: EditVersion,
    pub three_way_merge: String,
    pub article_id: i32,
    pub previous_version_id: EditVersion,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "ssr", diesel(table_name = instance, check_for_backend(diesel::pg::Pg)))]
pub struct DbInstance {
    pub id: i32,
    pub domain: String,
    #[cfg(feature = "ssr")]
    pub ap_id: ObjectId<DbInstance>,
    #[cfg(not(feature = "ssr"))]
    pub ap_id: String,
    pub description: Option<String>,
    #[cfg(feature = "ssr")]
    pub articles_url: CollectionId<DbArticleCollection>,
    #[cfg(not(feature = "ssr"))]
    pub articles_url: String,
    pub inbox_url: String,
    #[serde(skip)]
    pub public_key: String,
    #[serde(skip)]
    pub private_key: Option<String>,
    #[serde(skip)]
    pub last_refreshed_at: DateTime<Utc>,
    pub local: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(Queryable))]
#[cfg_attr(feature = "ssr", diesel(table_name = article, check_for_backend(diesel::pg::Pg)))]
pub struct InstanceView {
    pub instance: DbInstance,
    pub followers: Vec<DbPerson>,
    pub registration_open: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GetUserData {
    pub name: String,
    pub domain: Option<String>,
}

#[test]
fn test_edit_versions() {
    let default = EditVersion::default();
    assert_eq!("e3b0c44298fc1c149afbf4c8996fb924", default.hash());

    let version = EditVersion::new("test");
    assert_eq!("9f86d081884c7d659a2feaa0c55ad015", version.hash());
}
