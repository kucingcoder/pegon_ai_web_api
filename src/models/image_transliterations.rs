use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "image_transliterations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Binary(16)")]
    pub id: Uuid,
    #[sea_orm(column_type = "Binary(16)")]
    pub id_user: Uuid,
    #[sea_orm(unique)]
    pub image: String,
    pub title: String,
    #[sea_orm(column_type = "Text")]
    pub result: String,
    pub created_at: Option<DateTimeUtc>,
    pub updated_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::IdUser",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::users::Entity> for Entity {
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

        if insert {
            // Generate UUID otomatis jika belum di-set
            if self.id.is_not_set() {
                self.id = Set(Uuid::new_v4());
            }

            // Isi created_at saat data baru dibuat
            self.created_at = Set(Some(now));
        }

        // Selalu update updated_at (baik saat insert maupun update)
        self.updated_at = Set(Some(now));

        Ok(self)
    }
}
