terraform {
  required_version = ">= 1.8.0"

  backend "s3" {
    bucket                      = "buildry-tofu-state"
    key                         = "cloudflare/terraform.tfstate"
    region                      = "auto"
    endpoints                   = { s3 = "https://0efa3055e899dfc244b216554a334cf2.r2.cloudflarestorage.com" }
    use_path_style              = true
    use_lockfile                = true
    skip_credentials_validation = true
    skip_metadata_api_check     = true
    skip_region_validation      = true
    skip_requesting_account_id  = true
    skip_s3_checksum            = true
  }

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "5.22.0"
    }
  }
}

provider "cloudflare" {}
