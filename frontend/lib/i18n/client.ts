"use client";

import { useEffect, useState } from "react";
import { useCookies } from "react-cookie";
import { createInstance } from "i18next";
import { initReactI18next, useTranslation as useTranslationOrg } from "react-i18next";
import resourcesToBackend from "i18next-resources-to-backend";
import LanguageDetector from "i18next-browser-languagedetector";

import { fallbackLng, getOptions, languages } from "./settings";

const cookieName = "i18next";

function getI18nextOptions(lng = fallbackLng, ns = "common") {
  return {
    ...getOptions(lng, ns),
    lng,
  };
}

// Initialise i18next client instance once
const i18n = createInstance();

i18n
  .use(initReactI18next)
  .use(LanguageDetector)
  .use(
    resourcesToBackend(
      (language: string, namespace: string) => import(`../../public/locales/${language}/${namespace}.json`)
    )
  )
  .init(getI18nextOptions());

export function useTranslation(lng: string = fallbackLng, ns = "common") {
  const [cookies, setCookie] = useCookies([cookieName]);
  const [activeLng, setActiveLng] = useState(i18n.resolvedLanguage || lng);

  const ret = useTranslationOrg(ns);

  useEffect(() => {
    if (activeLng === lng) return;

    setActiveLng(lng);
    i18n.changeLanguage(lng);
  }, [lng, activeLng]);

  useEffect(() => {
    if (!lng) return; // guard against empty language values
    if (cookies[cookieName] === lng) return;

    setCookie(cookieName, lng, { path: "/" });
  }, [lng, setCookie]);

  return ret;
}

export { languages };
