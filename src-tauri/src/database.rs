use tauri_plugin_sql::{Migration, MigrationKind};

pub fn downloads_migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "create_downloads_table",
        sql: "CREATE TABLE IF NOT EXISTS downloads (
            gid TEXT PRIMARY KEY,
            fid TEXT NOT NULL,
            name TEXT NOT NULL,
            pick_code TEXT NOT NULL,
            size INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'active',
            progress REAL NOT NULL DEFAULT 0,
            path TEXT,
            download_speed INTEGER NOT NULL DEFAULT 0,
            eta INTEGER,
            error_message TEXT,
            error_code TEXT,
            created_at INTEGER,
            completed_at INTEGER,
            is_folder INTEGER NOT NULL DEFAULT 0,
            is_collecting INTEGER NOT NULL DEFAULT 0,
            parent_gid TEXT,
            total_files INTEGER,
            completed_files INTEGER,
            failed_files INTEGER
        );",
        kind: MigrationKind::Up,
    }]
}

pub fn uploads_migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "create_uploads_table",
        sql: "CREATE TABLE IF NOT EXISTS uploads (
            id TEXT PRIMARY KEY,
            file_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_size INTEGER NOT NULL DEFAULT 0,
            target_cid TEXT NOT NULL DEFAULT '0',
            target_path TEXT,
            sha1 TEXT,
            pre_sha1 TEXT,
            pick_code TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            progress REAL NOT NULL DEFAULT 0,
            upload_speed INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            created_at INTEGER,
            completed_at INTEGER,
            is_folder INTEGER NOT NULL DEFAULT 0,
            parent_id TEXT,
            total_files INTEGER,
            completed_files INTEGER,
            failed_files INTEGER,
            oss_bucket TEXT,
            oss_object TEXT,
            oss_endpoint TEXT,
            callback TEXT,
            callback_var TEXT,
            uploaded_size INTEGER NOT NULL DEFAULT 0,
            file_id TEXT,
            oss_upload_id TEXT
        );",
        kind: MigrationKind::Up,
    }]
}
