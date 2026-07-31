output "tunnel_id" {
  value = cloudflare_zero_trust_tunnel_cloudflared.home.id
}

output "tunnel_token" {
  value     = data.cloudflare_zero_trust_tunnel_cloudflared_token.home.token
  sensitive = true
}

output "application_url" {
  value = "https://${var.application_hostname}"
}
