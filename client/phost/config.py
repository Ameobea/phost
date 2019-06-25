import os
import pathlib
import toml

DEFAULT_CONF = {
    "api_server_url": "http://localhost:8000",
    "hosting_base_url": "ameo.design",
    "hosting_protocol": "https",
    "username": "user",
    "password": "pass",
}

CONFIG_DIR_PATH = os.path.join(os.path.expanduser("~"), ".phost")
CONF_FILE_PATH = os.path.join(CONFIG_DIR_PATH, "conf.toml")
COOKIE_FILE_PATH = os.path.join(CONFIG_DIR_PATH, "cookies.toml")


def load_cookies() -> dict:
    if not os.path.isfile(COOKIE_FILE_PATH):
        return {}

    with open(COOKIE_FILE_PATH) as f:
        return toml.loads(f.read())


def save_cookies(cookies: dict):
    with open(COOKIE_FILE_PATH, "w") as f:
        f.write(toml.dumps(cookies))


def init_config(conf_file_path):
    if not os.path.isfile(conf_file_path):
        pathlib.Path(os.path.dirname(conf_file_path)).mkdir(parents=False, exist_ok=True)

        # Initialize config file with default config if it's empty
        default_conf_toml = toml.dumps(DEFAULT_CONF)
        with open(conf_file_path, "w") as f:
            f.write(default_conf_toml)
            print(
                f"Created default config file: {conf_file_path}; you must fill in the values yourself."
            )
            exit(1)

    return open(conf_file_path, "r")


def load_conf(conf_file) -> dict:

    conf_toml = conf_file.read()
    conf_file.close()

    try:
        return toml.loads(conf_toml)
    except toml.TomlDecodeError:
        print("Error reading supplied config file!")
        exit(1)
