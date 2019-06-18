table! {
    auth_group (id) {
        id -> Integer,
        name -> Varchar,
    }
}

table! {
    auth_group_permissions (id) {
        id -> Integer,
        group_id -> Integer,
        permission_id -> Integer,
    }
}

table! {
    auth_permission (id) {
        id -> Integer,
        name -> Varchar,
        content_type_id -> Integer,
        codename -> Varchar,
    }
}

table! {
    auth_user (id) {
        id -> Integer,
        password -> Varchar,
        last_login -> Nullable<Datetime>,
        is_superuser -> Bool,
        username -> Varchar,
        first_name -> Varchar,
        last_name -> Varchar,
        email -> Varchar,
        is_staff -> Bool,
        is_active -> Bool,
        date_joined -> Datetime,
    }
}

table! {
    auth_user_groups (id) {
        id -> Integer,
        user_id -> Integer,
        group_id -> Integer,
    }
}

table! {
    auth_user_user_permissions (id) {
        id -> Integer,
        user_id -> Integer,
        permission_id -> Integer,
    }
}

table! {
    django_admin_log (id) {
        id -> Integer,
        action_time -> Datetime,
        object_id -> Nullable<Longtext>,
        object_repr -> Varchar,
        action_flag -> Unsigned<Smallint>,
        change_message -> Longtext,
        content_type_id -> Nullable<Integer>,
        user_id -> Integer,
    }
}

table! {
    django_content_type (id) {
        id -> Integer,
        app_label -> Varchar,
        model -> Varchar,
    }
}

table! {
    django_migrations (id) {
        id -> Integer,
        app -> Varchar,
        name -> Varchar,
        applied -> Datetime,
    }
}

table! {
    django_session (session_key) {
        session_key -> Varchar,
        session_data -> Longtext,
        expire_date -> Datetime,
    }
}

table! {
    serversite_deploymentcategory (id) {
        id -> Integer,
        category -> Varchar,
    }
}

table! {
    serversite_deploymentversion (id) {
        id -> Integer,
        version -> Varchar,
        created_on -> Datetime,
        deployment_id -> Char,
        active -> Bool,
    }
}

table! {
    serversite_proxydeployment (id) {
        id -> Char,
        name -> Varchar,
        subdomain -> Varchar,
        use_cors_headers -> Bool,
        destination_address -> Longtext,
        created_on -> Datetime,
    }
}

table! {
    serversite_staticdeployment (id) {
        id -> Char,
        name -> Varchar,
        subdomain -> Varchar,
        created_on -> Datetime,
        not_found_document -> Nullable<Longtext>,
    }
}

table! {
    serversite_staticdeployment_categories (id) {
        id -> Integer,
        staticdeployment_id -> Char,
        deploymentcategory_id -> Integer,
    }
}

table! {
    static_deployments (id) {
        id -> Bigint,
        deployment_name -> Varchar,
        subdomain -> Varchar,
    }
}

joinable!(auth_group_permissions -> auth_group (group_id));
joinable!(auth_group_permissions -> auth_permission (permission_id));
joinable!(auth_permission -> django_content_type (content_type_id));
joinable!(auth_user_groups -> auth_group (group_id));
joinable!(auth_user_groups -> auth_user (user_id));
joinable!(auth_user_user_permissions -> auth_permission (permission_id));
joinable!(auth_user_user_permissions -> auth_user (user_id));
joinable!(django_admin_log -> auth_user (user_id));
joinable!(django_admin_log -> django_content_type (content_type_id));
joinable!(serversite_deploymentversion -> serversite_staticdeployment (deployment_id));
joinable!(serversite_staticdeployment_categories -> serversite_deploymentcategory (deploymentcategory_id));
joinable!(serversite_staticdeployment_categories -> serversite_staticdeployment (staticdeployment_id));

allow_tables_to_appear_in_same_query!(
    auth_group,
    auth_group_permissions,
    auth_permission,
    auth_user,
    auth_user_groups,
    auth_user_user_permissions,
    django_admin_log,
    django_content_type,
    django_migrations,
    django_session,
    serversite_deploymentcategory,
    serversite_deploymentversion,
    serversite_proxydeployment,
    serversite_staticdeployment,
    serversite_staticdeployment_categories,
    static_deployments,
);
