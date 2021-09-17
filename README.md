# embed-py-env
CLI app for creating a portable embedded python environment for Windows.

## Why?

Because sometimes [virtualenvs just don't work right](https://gist.github.com/krscott/7b946c1c4f81291ede88b0b7de0e0fe6)?

## Usage

This will download the Embedded Python zip file for the current 
version of python in your PATH, unzip it into "myenv/", then install
all pip packages in requirements.txt.

```
cargo run -- myenv -r requirements.txt
```
