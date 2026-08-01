.PHONY: bootstrap bootstrap-cloudflare migrate-cloudflare-state status argocd-password

export KUBECONFIG := $(HOME)/.kube/config

bootstrap:
	sudo TARGET_USER="$${USER}" ./scripts/bootstrap.sh

bootstrap-cloudflare:
	./scripts/bootstrap-cloudflare.sh

migrate-cloudflare-state:
	./scripts/migrate-cloudflare-state-to-r2.sh

status:
	./scripts/status.sh

argocd-password:
	kubectl -n argocd get secret argocd-initial-admin-secret \
		-o jsonpath='{.data.password}' | base64 --decode; echo
