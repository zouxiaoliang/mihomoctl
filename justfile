alias r := run
alias b := build
alias d := dev

run *args:
	cargo run -p mihomoctl -- {{ args }}

reset_terminal:
	pkill mihomoctl && stty sane && stty cooked

dev:
	cargo watch -x 'check -p mihomoctl > /dev/null 2>&1 ' -s 'touch .trigger' > /dev/null &
	cargo watch --no-gitignore -w .trigger -x 'run -p mihomoctl'

build:
	cargo build --release

release os: build
	#!/usr/bin/env bash
	pushd target/release
	rm mihomoctl*.d
	mv mihomoctl-tui* mihomoctl-tui-{{ os }}
	mv mihomoctl* mihomoctl-{{ os }}
	popd

test *args:
	cargo test -- {{ args }} --nocapture