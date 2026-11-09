use crate::{
    icon::Icon,
    workspace::{strings, APPS, USER},
};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
static COLLAPSED: GlobalSignal<bool> =
    Signal::global(|| LocalStorage::get::<bool>("buildry.sidebar.collapsed").unwrap_or(false));
static PROPERTY_COUNT: GlobalSignal<Option<usize>> = Signal::global(|| None);
pub fn set_property_count(count: usize) {
    PROPERTY_COUNT.signal().set(Some(count));
}
// Every active user can create resources; individual editing uses server capabilities.
pub fn can_create_resources() -> bool {
    USER().is_some()
}
#[component]
pub fn Sidebar(active: &'static str) -> Element {
    let mut collapsed = COLLAPSED.signal();
    let mut more = use_signal(|| false);
    let ids = USER()
        .map(|u| strings(&u, "installed_apps"))
        .unwrap_or_default();
    let installed = ids
        .iter()
        .filter_map(|id| APPS.iter().find(|a| a.0 == id).copied())
        .collect::<Vec<_>>();
    rsx! {aside{class:if collapsed(){"sidebar collapsed"}else{"sidebar"},
    div{class:"sidebar-header",div{class:"brand",img{class:"brand-symbol",src:"/icons/buildry-mark.svg?v=20261017-vibrant",alt:""}span{class:"brand-name","Buildry"}}
    crate::form_button::FormButton{variant:crate::form_button::ButtonVariant::Icon,class:"sidebar-toggle",r#type:"button",aria_label:if collapsed(){"Expand sidebar"}else{"Collapse sidebar"},onclick:move|_|{let next=!collapsed();collapsed.set(next);let _=LocalStorage::set("buildry.sidebar.collapsed",next);},if collapsed(){Icon{name:"chevron-right"}}else{Icon{name:"chevron-left"}}}}
    for(index,(_,label,path,icon))in installed.iter().copied().enumerate(){a{href:path,class:format!("nav-item{}{}",if active==path{" active"}else{""},if index>=4{" nav-mobile-hidden"}else{""}),title:label,aria_current:if active==path{"page"}else{"false"},Icon{name:icon}span{class:"nav-text","{label}"}if path=="/properties"{if let Some(count)=PROPERTY_COUNT(){span{class:"nav-count","{count}"}}}}}
    crate::form_button::FormButton{class:"nav-item mobile-nav-more",r#type:"button",aria_label:"More navigation",aria_expanded:more(),onclick:move|_|more.set(!more()),Icon{name:"menu"}span{class:"nav-text","More"}}
    div{class:"nav-section-divider",role:"separator"}
    for(path,label,icon)in[("/apps","Apps","plus"),("/contacts","Contacts","users"),("/account","Account","users")]{a{href:path,class:format!("nav-item workspace-fixed-nav{}",if active==path{" active"}else{""}),title:label,Icon{name:icon}span{class:"nav-text","{label}"}}}
    if USER().is_some_and(|u|u["role"]=="admin"){a{href:"/access",class:"nav-item workspace-fixed-nav",title:"Administration",Icon{name:"users"}span{class:"nav-text","Administration"}}}
    crate::app_form::AppForm{class:"nav-logout",method:"post",action:"/logout",crate::form_button::FormButton{class:"nav-item",r#type:"submit",title:"Sign out",Icon{name:"logout"}span{class:"nav-text","Sign out"}}}}
    if more(){crate::shared_ui::Dialog{label:"Navigation",on_close:move|_|more.set(false),swipe_to_close:true,card_class:"workspace-mobile-menu",nav{for(_,label,path,icon)in installed.iter().copied(){a{class:"workspace-setting-row",href:path,Icon{name:icon}span{"{label}"}}}for(path,label)in[("/apps","Apps"),("/contacts","Contacts"),("/account","Account")]{a{class:"workspace-setting-row",href:path,"{label}"}}}}}
    }
}
