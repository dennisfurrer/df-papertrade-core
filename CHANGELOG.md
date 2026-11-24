# Changelog

All notable changes to df-papertrade-core.

## 0.0.13-beta

- Depends on df-hyperliquid-core 0.1.1, the first stable release, instead of 0.0.10-beta.
- A signed intent carries a `bytes` field as `0x` and lowercase hex. df-hyperliquid-core 0.1 added `Field::Bytes`; no papertrade intent uses it yet.
- `Client::set_builder` and `Client::current_builder`: the builder code that registrations name can be changed at runtime, and clones of a client share it.

## 0.0.12-beta

- Contract set 2 and builder codes.

## 0.0.11-beta and earlier

See the [GitHub repository](https://github.com/dennisfurrer/df-papertrade-core) history.
