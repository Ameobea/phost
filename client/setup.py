from setuptools import setup

setup(
    name="phost-cli",
    version="0.1.0",
    packages=["phost-cli"],
    entry_points={"console_scripts": ["phost-cli = phost-cli.__main__:main"]},
    install_requires=["npyscreen", "click", "requests", "toml", "terminaltable", "python-dateutil"],
    license="MIT",
    url="http://github.com/ameobea/project-hoster",
    author="Casey Primozic (Ameo)",
    author_email="me@ameo.link",
)
