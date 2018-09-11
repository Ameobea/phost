from setuptools import setup

setup(
    name="phost",
    version="0.1.0",
    packages=["phost"],
    entry_points={"console_scripts": ["phost=phost.__main__:main"]},
    install_requires=[
        "npyscreen",
        "click",
        "requests",
        "toml",
        "terminaltables",
        "python-dateutil",
    ],
    license="MIT",
    url="http://github.com/ameobea/project-hoster",
    author="Casey Primozic (Ameo)",
    author_email="me@ameo.link",
)
