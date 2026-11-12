//! Server-rendered adapters for the shared form controls. Native submission keeps
//! authentication and private signing usable without loading the WASM application.
//! Attribute fragments are trusted literals supplied only by these renderers.
use crate::auth::html_escape;

pub fn input(id: &str, name: &str, kind: &str, autocomplete: &str, attributes: &str) -> String {
    format!(
        r#"<input class="shared-form-input" id="{}" name="{}" type="{}" autocomplete="{}" {attributes}>"#,
        html_escape(id),
        html_escape(name),
        html_escape(kind),
        html_escape(autocomplete)
    )
}

pub fn field(label: &str, id: &str, control: &str) -> String {
    format!(
        r#"<div class="field"><label for="{}">{}</label>{control}</div>"#,
        html_escape(id),
        html_escape(label)
    )
}

pub fn button(label: &str, attributes: &str) -> String {
    action_button(label, "", attributes)
}

pub fn action_button(label: &str, class: &str, attributes: &str) -> String {
    format!(
        r#"<button class="shared-form-button {}" {attributes}>{}</button>"#,
        html_escape(class),
        html_escape(label)
    )
}

pub fn form(action: &str, class: &str, attributes: &str, content: &str) -> String {
    format!(
        r#"<form class="app-form {}" method="post" action="{}" {attributes}>{content}</form>"#,
        html_escape(class),
        html_escape(action)
    )
}

pub fn auth_fields(signup: bool) -> String {
    let mut fields = String::new();
    if signup {
        fields += &field(
            "First name",
            "first-name",
            &input(
                "first-name",
                "first_name",
                "text",
                "given-name",
                "maxlength='60' required autofocus",
            ),
        );
        fields += &field(
            "Last name",
            "last-name",
            &input(
                "last-name",
                "last_name",
                "text",
                "family-name",
                "maxlength='60' required",
            ),
        );
    }
    fields += &field(
        "Email",
        "email",
        &input(
            "email",
            "email",
            "email",
            if signup { "email" } else { "username" },
            if signup {
                "maxlength='254' required"
            } else {
                "required autofocus"
            },
        ),
    );
    fields += &field(
        "Password",
        "password",
        &input(
            "password",
            "password",
            "password",
            if signup {
                "new-password"
            } else {
                "current-password"
            },
            if signup {
                "minlength='12' required"
            } else {
                "required"
            },
        ),
    );
    if signup {
        fields += &password_confirmation();
    }
    fields
}

fn password_confirmation() -> String {
    field(
        "Confirm password",
        "confirm-password",
        &input(
            "confirm-password",
            "confirm_password",
            "password",
            "new-password",
            "minlength='12' required",
        ),
    )
}

pub fn email_form(action: &str, label: &str) -> String {
    form(
        action,
        "",
        "",
        &format!(
            "{}{}",
            field(
                "Email",
                "email",
                &input("email", "email", "email", "email", "required")
            ),
            button(label, "type='submit'")
        ),
    )
}

pub fn password_form(action: &str) -> String {
    form(
        action,
        "",
        "",
        &format!(
            "{}{}{}",
            field(
                "Password",
                "password",
                &input(
                    "password",
                    "password",
                    "password",
                    "new-password",
                    "minlength='12' required"
                )
            ),
            password_confirmation(),
            button("Set password", "type='submit'")
        ),
    )
}

pub fn signature_form(
    action: &str,
    kind: &str,
    name: &str,
    prefill: bool,
    consent: &str,
    submit: &str,
) -> String {
    let id = if kind == "tenant" {
        "legal-signature"
    } else {
        "landlord-signature"
    };
    let value = format!(
        "required maxlength='120' {}='{}'",
        if prefill { "value" } else { "placeholder" },
        html_escape(name)
    );
    form(action, "sign-form", &format!("data-signature-form data-kind='{}'", html_escape(kind)), &format!(
        "<label for='{id}'>Your legal name</label>{}<label class='sign-consent'><input class='shared-form-checkbox' type='checkbox' name='consent' value='yes' required><span>{}</span></label>{}<div class='sign-preview' data-sign-preview hidden><span>Signature preview</span><strong></strong></div>{}",
        input(id, "signature", "text", "name", &value), html_escape(consent), button("Preview signature", "type='button' data-preview-signature"), button(submit, "type='submit' data-submit-signature hidden")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentication_preserves_native_validation_and_submission_names() {
        let signup = auth_fields(true);
        for name in [
            "first_name",
            "last_name",
            "email",
            "password",
            "confirm_password",
        ] {
            assert!(signup.contains(&format!("name=\"{name}\"")));
        }
        assert!(signup.contains("minlength='12' required"));
        assert!(auth_fields(false).contains("autocomplete=\"current-password\""));
        assert!(password_form("/set-password/token").contains("action=\"/set-password/token\""));
    }

    #[test]
    fn signing_adapters_escape_names_and_preserve_preview_consent_contract() {
        let html = signature_form(
            "/lease/test",
            "tenant",
            "O'Neil <test>",
            false,
            "I agree.",
            "Submit signature",
        );
        assert!(html.contains("placeholder='O&#39;Neil &lt;test&gt;'"));
        assert!(html.contains("data-signature-form data-kind='tenant'"));
        assert!(html.contains("name='consent' value='yes' required"));
        assert!(html.contains("type='submit' data-submit-signature hidden"));
        assert!(html.contains("id=\"legal-signature\""));
        assert!(
            signature_form("", "landlord", "Signer", true, "I agree.", "Sign")
                .contains("value='Signer'")
        );
    }
}
