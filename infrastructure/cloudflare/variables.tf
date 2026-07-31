variable "cloudflare_account_id" {
  description = "Cloudflare account containing the Tunnel."
  type        = string
  default     = "0efa3055e899dfc244b216554a334cf2"
}

variable "cloudflare_zone_id" {
  description = "Cloudflare zone for buildry.ca."
  type        = string
  default     = "a33409b4985bf31c33627da5114ff1c7"
}

variable "application_hostname" {
  description = "Public hostname routed through the Tunnel."
  type        = string
  default     = "app.buildry.ca"
}
