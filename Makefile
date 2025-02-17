.PHONY: docker docker-dev push-image push-image-dev

docker:
	-docker image rm jaeger-anomaly-detection
	DOCKER_BUILDKIT=1 docker build --ssh default --target image-release -t jaeger-anomaly-detection .

docker-dev:
	-docker image rm jaeger-anomaly-detection
	DOCKER_BUILDKIT=1 docker build --ssh default --target image-dev -t jaeger-anomaly-detection .

push-image: docker
	docker tag jaeger-anomaly-detection:latest gitea.contc/continuousc/jaeger-anomaly-detection:latest
	docker push gitea.contc/continuousc/jaeger-anomaly-detection:latest

push-image-dev: docker-dev
	docker tag jaeger-anomaly-detection:latest gitea.contc/continuousc/jaeger-anomaly-detection:dev-latest
	docker push gitea.contc/continuousc/jaeger-anomaly-detection:dev-latest

openapi.json: Cargo.toml Cargo.lock $(shell find src/ -name '*.rs')
	cargo run -- --spec > openapi.json
