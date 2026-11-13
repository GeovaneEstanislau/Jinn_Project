# Title: 🇪🇸 Llamado a colaboradores y revisión técnica: Comunidad Hispanohablante (README.es.md)
# Labels: documentation, i18n, help wanted, good first issue

¡Bienvenidos a la comunidad de **Jinn OS**!

Hemos publicado la versión oficial en español de la documentación central de nuestro proyecto: [**`README.es.md`**](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.es.md).

### 🎯 Objetivo de este Issue
**Jinn OS** es un sistema operativo con arquitectura de **microkernel x86_64**, minimalista y preemptivo, escrito completamente desde cero en Rust moderno (`#![no_std]`, bare-metal). El sistema garantiza el principio de mínimo privilegio ejecutando únicamente las rutinas indispensables en **Ring 0**, mientras que los controladores de red, sistemas de archivos y el compositor gráfico **Aladdin Desktop** residen de manera aislada en el espacio de usuario (**Ring 3**).

Queremos invitar a la gran comunidad hispanohablante de desarrolladores de software, ingenieros de sistemas y entusiastas de Rust a participar activamente en la evolución del proyecto.

### 🔍 ¿Cómo puedes ayudar?
1. **Revisión de Precisión Técnica y Redacción:**
   - Validar que los conceptos de bajo nivel (*paginación en 4 niveles PML4*, *seguridad W^X*, *preempción PIT a 100Hz*, *comunicación IPC libre de bloqueos*, *VFS con persistencia transaccional* y *compositor Aladdin Desktop en Ring 3*) mantengan una traducción técnica precisa e idiomática.
   - Reportar errores tipográficos o sugerir mejoras en la redacción.
2. **Desarrollo del Kernel y del Ecosistema Ring 3:**
   - Escribir y probar nuevos controladores de hardware (VirtIO-Net, Intel E1000, NVMe, AHCI, USB).
   - Colaborar en las aplicaciones nativas de Ring 3 (el navegador **Jinn Browser**, el terminal **Aladdin Shell** y la billetera Web3 **Aladdin Wallet**).
   - Probar Jinn OS en máquinas reales o entornos virtuales (QEMU, VirtualBox) y reportar métricas de estabilidad.
3. **Preguntas, Discusión e Ideas:**
   - ¡Deja un comentario aquí abajo con tus dudas, propuestas arquitectónicas o sugerencias!

### 📚 Primeros Pasos
- Lee la documentación: [`README.es.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.es.md)
- Pautas de Contribución: [`CONTRIBUTING.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CONTRIBUTING.md)
- Acuerdo de Licencia de Colaborador: [`CLA.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CLA.md)

¡Muchas gracias por ayudarnos a construir una base de cómputo soberana, segura y transparente en Rust!
