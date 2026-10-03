.PHONY: bootstrap bootstrap-cloudflare status argocd-password publish-property-catalog

export KUBECONFIG := $(HOME)/.kube/config

bootstrap:
	sudo TARGET_USER="$${USER}" ./scripts/bootstrap.sh

bootstrap-cloudflare:
	./scripts/bootstrap-cloudflare.sh

status:
	./scripts/status.sh

argocd-password:
	kubectl -n argocd get secret argocd-initial-admin-secret \
		-o jsonpath='{.data.password}' | base64 --decode; echo

publish-property-catalog:
	./scripts/publish-property-catalog.sh
