
export here := $(PWD)

build-lib:
	cd $$here/event-engine; cargo build

build-ex-rust-app:
	cd $$here; docker build -f Dockerfile-ex-app  -t tapis/event-engine-ex-app .

build-python-base-image:
	cd $$here/pyevents; docker build -t tapis/pyevents .

build-ex-python-plugin:
	cd $$here/example/pyplugin; docker build -t tapis/events-engine-ex-pyplugin .

build: build-lib build-ex-rust-app build-python-base-image build-ex-python-plugin
	
up: build
	cd $$here/example; docker-compose up 