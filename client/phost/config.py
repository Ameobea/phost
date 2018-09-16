import os
import pathlib
import toml

DEFAULT_CONF = {
    "api_server_url": "http://localhost:8000",
    "hosting_base_url": "ameo.design",
    "hosting_protocol": "https",
}


def init_config():
    config_dir_path = os.path.join(os.path.expanduser("~"), ".phost")
    pathlib.Path(config_dir_path).mkdir(parents=False, exist_ok=True)

    # Initialize config file with default config if it's empty
    conf_file_path = os.path.join(config_dir_path, "conf.toml")
    if not os.path.isfile(conf_file_path):
        default_conf_toml = toml.dumps(DEFAULT_CONF)
        with open(conf_file_path, "w") as f:
            f.write(default_conf_toml)

    return open(conf_file_path, "r")


def load_conf(conf_file) -> dict:
    conf_file = conf_file or init_config()
    conf_toml = conf_file.read()
    conf_file.close()

    try:
        return toml.loads(conf_toml)
    except toml.TomlDecodeError:
        print("Error reading supplied config file!")
        exit(1)
