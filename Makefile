.PHONY: bootstrap status argocd-password

export KUBECONFIG := $(HOME)/.kube/config

bootstrap:
	sudo TARGET_USER="$${USER}" ./scripts/bootstrap.sh

status:
	./scripts/status.sh

argocd-password:
	kubectl -n argocd get secret argocd-initial-admin-secret \
		-o jsonpath='{.data.password}' | base64 --decode; echo
