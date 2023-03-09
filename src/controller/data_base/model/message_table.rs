//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.0

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "message_table")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(column_type = "custom(\"LONGTEXT\")")]
    pub message: String,
    pub r#type: i32,
    pub created_time: Option<DateTime>,
    pub updated_time: Option<DateTime>,
    pub identity_token: String,
    pub room_token: String,
    pub owner_name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}