import os
import pathlib
import toml

DEFAULT_CONF = {
    "api_server_url": "http://localhost:8000",
    "hosting_base_url": "ameo.design",
    "hosting_protocol": "https",
}


def init_config() -> str:
    config_dir_path = os.path.join(os.path.expanduser("~"), ".phost")
    pathlib.Path(config_dir_path).mkdir(parents=False, exist_ok=True)

    # Initialize config file with default config if it's empty
    conf_file_path = os.path.join(config_dir_path, "conf.toml")
    if not os.path.isfile(conf_file_path):
        default_conf_toml = toml.dumps(DEFAULT_CONF)
        with open(conf_file_path, "w") as f:
            f.write(default_conf_toml)

    return conf_file_path


def load_conf() -> dict:
    # Initialize the config directory and config file with defaults if they don't exist
    conf_file_path = init_config()

    conf_toml = None
    with open(conf_file_path, "r") as f:
        conf_toml = f.read()

    return toml.loads(conf_toml)
