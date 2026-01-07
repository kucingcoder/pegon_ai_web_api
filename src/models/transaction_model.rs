use super::sea_orm_active_enums::Status;
use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Binary(16)")]
    pub id: Uuid,
    #[sea_orm(column_type = "Binary(16)")]
    pub id_user: Uuid,
    #[sea_orm(column_type = "Binary(16)")]
    pub id_midtrans: Uuid,
    #[sea_orm(column_type = "Text", unique)]
    pub qr_code: String,
    pub title: String,
    pub value: i32,
    pub status: Status,
    pub created_at: Option<DateTimeUtc>,
    pub updated_at: Option<DateTimeUtc>,
    pub expired_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user_model::Entity",
        from = "Column::IdUser",
        to = "super::user_model::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::user_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = Utc::now();

        // Logika saat Insert (Data Baru)
        if insert {
            // Set created_at ke waktu sekarang
            self.created_at = Set(Some(now));
        }

        // Selalu update updated_at ke waktu sekarang
        self.updated_at = Set(Some(now));

        Ok(self)
    }
}
