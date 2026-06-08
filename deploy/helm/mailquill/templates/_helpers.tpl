{{- define "mailquill.fullname" -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "mailquill.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
{{ include "mailquill.selectorLabels" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "mailquill.selectorLabels" -}}
app.kubernetes.io/name: mailquill
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}
