# embed-py-env
CLI app for creating a portable embedded python environment for Windows.

## Usage

This will download the Embedded Python zip file for the current 
version of python in your PATH, unzip it into "myenv/", then install
all pip packages in requirements.txt.

```
cargo run -- myenv -r requirements.txt
```
